//! 模型来源解析：本地路径 → 缓存 → 远程下载，并做 sha256 校验。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tatr_core::TatrError;

/// 默认模型下载地址（GitHub Release 资产）。
pub const DEFAULT_MODEL_URL: &str = "https://github.com/hexai-cn/tatr/releases/download/models-v1/table_detector.onnx";

/// 默认模型 sha256（防止下载损坏/被替换）。
pub const DEFAULT_MODEL_SHA256: &str = "cdee2c25b48cfe287703d41b9314a7b458ec8dd757815d73c133f90a2dcdab49";

/// 模型来源。
#[derive(Debug, Clone)]
pub enum ModelSource {
    /// 直接使用给定文件（不校验哈希）。
    LocalFile(PathBuf),
    /// 使用缓存目录中的文件；不存在则从 `url` 下载并校验 `sha256`。
    Download {
        /// 下载地址。
        url: String,
        /// 期望的 sha256（十六进制小写）。
        sha256: String,
        /// 缓存目录；`None` 表示用平台默认缓存目录。
        cache_dir: Option<PathBuf>,
    },
}

impl Default for ModelSource {
    fn default() -> Self {
        Self::Download {
            url: DEFAULT_MODEL_URL.to_string(),
            sha256: DEFAULT_MODEL_SHA256.to_string(),
            cache_dir: None,
        }
    }
}

impl ModelSource {
    /// 解析为可用的本地文件路径。必要时下载。
    pub fn resolve(&self) -> Result<PathBuf, TatrError> {
        match self {
            Self::LocalFile(p) => {
                if !p.is_file() {
                    return Err(TatrError::InvalidConfig(format!("模型文件不存在: {}", p.display())));
                }
                Ok(p.clone())
            }
            Self::Download { url, sha256, cache_dir } => {
                let dir = cache_dir.clone().unwrap_or_else(default_cache_dir);
                fs::create_dir_all(&dir)
                    .map_err(|e| TatrError::InvalidConfig(format!("创建缓存目录失败 {}: {e}", dir.display())))?;
                let name = url.rsplit('/').next().unwrap_or("model.onnx");
                let dest = dir.join(name);
                if dest.is_file() {
                    if verify_sha256(&dest, sha256)? {
                        tracing::debug!(path = %dest.display(), "命中模型缓存");
                        return Ok(dest);
                    }
                    tracing::warn!(path = %dest.display(), "缓存模型哈希不符，重新下载");
                    let _ = fs::remove_file(&dest);
                }
                download(url, &dest)?;
                if !verify_sha256(&dest, sha256)? {
                    let _ = fs::remove_file(&dest);
                    return Err(TatrError::InvalidConfig(format!(
                        "下载的模型 sha256 与期望不符（期望 {sha256}）"
                    )));
                }
                Ok(dest)
            }
        }
    }

    /// 默认缓存目录（`~/.cache/tatr` 或平台等价路径）。
    #[must_use]
    pub fn default_cache_dir() -> PathBuf {
        default_cache_dir()
    }
}

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")).join("tatr")
}

/// 校验文件 sha256；返回是否匹配。
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<bool, TatrError> {
    let mut f =
        fs::File::open(path).map_err(|e| TatrError::InvalidConfig(format!("打开模型失败 {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| TatrError::InvalidConfig(format!("读取模型失败: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = hex::encode(hasher.finalize());
    Ok(got.eq_ignore_ascii_case(expected_hex))
}

/// 计算文件 sha256（十六进制小写）。
pub fn file_sha256(path: &Path) -> Result<String, TatrError> {
    let mut f =
        fs::File::open(path).map_err(|e| TatrError::InvalidConfig(format!("打开文件失败 {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| TatrError::InvalidConfig(format!("读取失败: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 最小依赖的 HTTPS 下载（先写临时文件再原子改名）。
fn download(url: &str, dest: &Path) -> Result<(), TatrError> {
    tracing::info!(%url, path = %dest.display(), "下载模型");
    let resp = ureq_get(url)?;
    let tmp = dest.with_extension("part");
    {
        let mut out = fs::File::create(&tmp).map_err(|e| TatrError::InvalidConfig(format!("创建临时文件失败: {e}")))?;
        out.write_all(&resp)
            .map_err(|e| TatrError::InvalidConfig(format!("写入模型失败: {e}")))?;
        out.flush().ok();
    }
    fs::rename(&tmp, dest).map_err(|e| TatrError::InvalidConfig(format!("重命名模型失败: {e}")))?;
    Ok(())
}

/// 通过外部 `curl` 取回 URL 内容，避免把 TLS 栈作为依赖引入。
///
/// 仓库只依赖 `ort`/`axum` 等必要项；模型下载是低频运维动作，用系统 `curl`
/// 比引入 rustls/reqwest 更省体积，也便于用户替换为企业内网代理。
fn ureq_get(url: &str) -> Result<Vec<u8>, TatrError> {
    let out = std::process::Command::new("curl")
        .args(["-fL", "--retry", "3", "--connect-timeout", "20", "-o", "-", url])
        .output()
        .map_err(|e| TatrError::InvalidConfig(format!("调用 curl 失败（请确认系统有 curl）: {e}")))?;
    if !out.status.success() {
        return Err(TatrError::InvalidConfig(format!(
            "下载失败 {}: curl 退出码 {:?}",
            url,
            out.status.code()
        )));
    }
    if out.stdout.is_empty() {
        return Err(TatrError::InvalidConfig(format!("下载内容为空: {url}")));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_file_source_rejects_missing_path() {
        let src = ModelSource::LocalFile(PathBuf::from("/definitely/not/here.onnx"));
        assert!(src.resolve().is_err());
    }

    #[test]
    fn sha256_matches_known_vector() {
        // 空文件 sha256 是公开已知值，用于固定实现正确性
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("empty.bin");
        fs::write(&p, b"").unwrap();
        assert_eq!(
            file_sha256(&p).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(verify_sha256(&p, "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855").unwrap());
        assert!(!verify_sha256(&p, "00").unwrap());
    }
}
