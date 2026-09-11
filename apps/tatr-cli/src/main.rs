//! # tatr — 表格检测命令行
//!
//! ```text
//! tatr detect <image>...            # 输出 JSON 到 stdout
//! tatr model info                   # 打印模型来源与线程配置
//! ```
//!
//! CPU 部署：默认按物理核数设置 intra-op 线程，可用 `TATR_THREADS` 覆盖。

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use tatr_core::DetectorConfig;
use tatr_engine::{EngineOptions, ModelSource, TableDetectionEngine};

#[derive(Parser, Debug)]
#[command(name = "tatr", version, about = "Table Transformer 表格检测 (CPU-first)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 检测图像中的表格
    Detect(DetectArgs),
    /// 模型相关子命令
    Model(ModelArgs),
}

#[derive(Args, Debug)]
struct ModelArgs {
    #[command(subcommand)]
    command: ModelCommand,
}

#[derive(Subcommand, Debug)]
enum ModelCommand {
    /// 解析并打印模型信息（不加载推理）
    Info {
        /// 本地模型路径；省略则使用下载来源
        #[arg(long)]
        model: Option<PathBuf>,
    },
    /// 下载模型到缓存目录并校验 sha256
    Fetch {
        /// 缓存目录
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
struct DetectArgs {
    /// 输入图像（PNG/JPEG/WebP/BMP/TIFF），可多个
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// 本地 ONNX 模型；省略则从缓存/远程获取
    #[arg(long)]
    model: Option<PathBuf>,

    /// 分数阈值
    #[arg(long, default_value_t = 0.5)]
    threshold: f32,

    /// 缩放后短边
    #[arg(long, default_value_t = 800)]
    short_side: u32,

    /// 缩放后长边上限
    #[arg(long, default_value_t = 800)]
    long_side: u32,

    /// 丢弃 `table rotated` 类（默认保留：实测零误检代价提升召回）
    #[arg(long, default_value_t = false)]
    drop_rotated: bool,

    /// NMS IoU 阈值；1.0 表示关闭（DETR 默认无需 NMS）
    #[arg(long, default_value_t = 1.0)]
    nms_iou: f32,

    /// 输出文件；省略则打印到 stdout
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// intra-op 线程数（默认物理核数；等价于设置 TATR_THREADS）
    #[arg(long)]
    threads: Option<usize>,
}

fn build_engine(model: Option<PathBuf>, threads: Option<usize>) -> Result<TableDetectionEngine> {
    let model = match model {
        Some(p) => ModelSource::LocalFile(p),
        None => ModelSource::default(),
    };
    let mut opts = EngineOptions {
        model,
        ..Default::default()
    };
    opts.session.intra_threads = threads;
    TableDetectionEngine::new(opts).context("初始化检测引擎失败（模型获取或 ONNX 契约不符）")
}

fn main() -> ExitCode {
    // 日志必须走 stderr：stdout 专供 JSON 输出，保证 `tatr detect ... | jq` 可用
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "tatr=info".into()))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Model(args) => match args.command {
            ModelCommand::Info { model } => {
                let src = match model {
                    Some(p) => ModelSource::LocalFile(p),
                    None => ModelSource::default(),
                };
                let resolved = src.resolve().context("解析模型路径失败")?;
                let sha = tatr_engine::model_sha256(&resolved).unwrap_or_else(|_| "<n/a>".into());
                let threads = tatr_engine::SessionConfig::default().resolve_threads();
                println!(
                    "{}",
                    serde_json::json!({
                        "model_path": resolved.display().to_string(),
                        "sha256": sha,
                        "intra_threads": threads,
                        "default_url": tatr_engine::DEFAULT_MODEL_URL,
                    })
                );
                Ok(())
            }
            ModelCommand::Fetch { cache_dir } => {
                let src = ModelSource::Download {
                    url: tatr_engine::DEFAULT_MODEL_URL.to_string(),
                    sha256: tatr_engine::DEFAULT_MODEL_SHA256.to_string(),
                    cache_dir,
                };
                let p = src.resolve().context("下载模型失败")?;
                println!("{}", p.display());
                Ok(())
            }
        },
        Command::Detect(args) => {
            let engine = build_engine(args.model.clone(), args.threads)?;
            let cfg = DetectorConfig {
                threshold: args.threshold,
                short_side: args.short_side,
                long_side: args.long_side,
                include_rotated: !args.drop_rotated,
                nms_iou: args.nms_iou,
                ..Default::default()
            };
            cfg.validate()?;

            let mut results = Vec::new();
            for path in &args.inputs {
                let img =
                    tatr_engine::decode_image_file(path).with_context(|| format!("读取图像 {}", path.display()))?;
                let r = engine
                    .detect(&img, &cfg)
                    .with_context(|| format!("检测 {}", path.display()))?;
                tracing::info!(
                    file = %path.display(),
                    tables = r.detections.len(),
                    rotated = r.rotated_count(),
                    ms = format!("{:.0}", r.elapsed_ms),
                    "detected"
                );
                results.push(serde_json::json!({
                    "image": path.display().to_string(),
                    "width": r.width,
                    "height": r.height,
                    "input_size": [r.input_size.0, r.input_size.1],
                    "elapsed_ms": (r.elapsed_ms * 100.0).round() / 100.0,
                    "detections": r.detections,
                }));
            }

            let doc = serde_json::json!({
                "engine": { "threads": engine.threads(), "model": engine.model_path().display().to_string() },
                "results": results,
            });
            let text = serde_json::to_string_pretty(&doc)?;
            match args.out {
                Some(p) => {
                    std::fs::write(&p, &text).with_context(|| format!("写入 {}", p.display()))?;
                    eprintln!("[json] -> {}", p.display());
                }
                None => println!("{text}"),
            }
            Ok(())
        }
    }
}
