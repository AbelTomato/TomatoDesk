//! 历史链接记录回填：只补缺失的标题与摘要。
//!
//! 默认 dry-run，只打印将要发生的变更；`--apply` 才写库，写前先做整库备份。
//! 命名规则与随手记的即时保存、后台补全共用 `link_label`，避免回填结果与日常写入不一致。

use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    process::ExitCode,
};

use reqwest::Url;
use tomatodesk_lib::{
    domain::Capture,
    infrastructure::{
        db::Database,
        link_label::{labels_from_url, labels_with_metadata},
        web_metadata::fetch_web_metadata,
    },
};

const PROD_IDENTIFIER: &str = "com.tomatodesk.app";
const DEV_IDENTIFIER: &str = "com.tomatodesk.app.dev";

const USAGE: &str = "\
用法：backfill_capture_labels [--db <path>] [--dev] [--apply] [--fetch] [--limit <n>]

默认 dry-run，只打印将要发生的变更；--apply 才写库。
  --db <path>  指定数据库文件，缺省用应用的默认数据目录
  --dev        使用开发版（com.tomatodesk.app.dev）的数据目录
  --apply      真正写库，写前用 VACUUM INTO 备份到库文件旁的 backup/
  --fetch      额外联网抓取页面标题与描述，默认只按 URL 推导
  --limit <n>  本次最多处理多少条
";

struct Options {
    db: Option<PathBuf>,
    dev: bool,
    apply: bool,
    fetch: bool,
    limit: Option<usize>,
}

impl Options {
    /// 返回 `Ok(None)` 表示用户要的是帮助信息。
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Self>, String> {
        let mut options = Self {
            db: None,
            dev: false,
            apply: false,
            fetch: false,
            limit: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--db" => {
                    let value = args.next().ok_or("--db 需要一个数据库路径")?;
                    options.db = Some(PathBuf::from(value));
                }
                "--dev" => options.dev = true,
                "--apply" => options.apply = true,
                "--fetch" => options.fetch = true,
                "--limit" => {
                    let value = args.next().ok_or("--limit 需要一个整数")?;
                    let limit = value
                        .parse::<usize>()
                        .map_err(|_| format!("--limit 需要正整数，收到 {value}"))?;
                    if limit == 0 {
                        return Err("--limit 至少为 1".into());
                    }
                    options.limit = Some(limit);
                }
                "--help" | "-h" => return Ok(None),
                other => return Err(format!("未知参数：{other}")),
            }
        }
        if options.db.is_some() && options.dev {
            return Err("--db 与 --dev 只能选一个".into());
        }
        Ok(Some(options))
    }

    fn database_path(&self) -> Result<PathBuf, String> {
        if let Some(path) = &self.db {
            return Ok(path.clone());
        }
        let app_data = env::var_os("APPDATA").ok_or("找不到 APPDATA，请用 --db 指定数据库路径")?;
        let identifier = if self.dev {
            DEV_IDENTIFIER
        } else {
            PROD_IDENTIFIER
        };
        Ok(PathBuf::from(app_data)
            .join(identifier)
            .join("tomatodesk.sqlite3"))
    }
}

fn main() -> ExitCode {
    let options = match Options::parse(env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("回填失败：{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &Options) -> Result<(), Box<dyn Error>> {
    let path = options.database_path()?;
    // 路径写错时不要新建一个空库再「成功」跑完：脚本只处理已存在的库。
    if !path.is_file() {
        return Err(format!("数据库不存在：{}，请用 --db 指定正确路径", path.display()).into());
    }

    let database = Database::open(&path)?;
    if options.apply {
        println!("提示：执行前请先退出 TomatoDesk，避免与应用争用写锁。");
        let target = backup_path(&path);
        database.snapshot_to(&target)?;
        println!("已备份到 {}", target.display());
    }

    let rows = database.captures().list_incomplete_links(options.limit)?;
    if rows.is_empty() {
        println!("没有需要补全的链接记录。");
        return Ok(());
    }

    // 只有需要联网时才建运行时：默认路径完全离线。
    let runtime = if options.fetch {
        Some(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
        )
    } else {
        None
    };

    let mut filled = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for capture in &rows {
        let Some(url_text) = capture.url.as_deref() else {
            skipped += 1;
            continue;
        };
        let Ok(url) = Url::parse(url_text) else {
            failed += 1;
            eprintln!("跳过 {}：URL 无法解析（{url_text}）", capture.id);
            continue;
        };
        let labels = match runtime.as_ref() {
            Some(runtime) => runtime
                .block_on(fetch_web_metadata(&url))
                .map(|metadata| labels_with_metadata(&url, &metadata))
                .unwrap_or_else(|| labels_from_url(&url)),
            None => labels_from_url(&url),
        };
        let title = fill_gap(capture.title.as_deref(), &labels.title);
        let summary = fill_gap(capture.summary.as_deref(), &labels.summary);

        if !options.apply {
            filled += 1;
            println!("将补全 {}", describe(capture, title, summary));
            continue;
        }
        match database
            .captures()
            .fill_missing_link_labels(&capture.id, &labels)
        {
            Ok(true) => {
                filled += 1;
                println!("已补全 {}", describe(capture, title, summary));
            }
            // 读到写之间已被应用补上：不算失败，也不再打印。
            Ok(false) => skipped += 1,
            Err(error) => {
                failed += 1;
                eprintln!("写入失败 {}：{error}", capture.id);
            }
        }
    }

    let verb = if options.apply { "补全" } else { "可补全" };
    println!(
        "扫描 {} 条，{verb} {filled} 条，跳过 {skipped} 条，失败 {failed} 条",
        rows.len()
    );
    if !options.apply && filled > 0 {
        println!("未写库；确认无误后加 --apply 执行。");
    }
    if failed > 0 {
        return Err(format!("{failed} 条处理失败").into());
    }
    Ok(())
}

/// 只列出本次真正会变化的字段，避免输出里出现「保留原值」的噪音。
fn describe(capture: &Capture, title: Option<&str>, summary: Option<&str>) -> String {
    let mut parts = vec![capture.id.clone()];
    if let Some(title) = title {
        parts.push(format!("标题 {title:?}"));
    }
    if let Some(summary) = summary {
        parts.push(format!("摘要 {summary:?}"));
    }
    parts.join("  ")
}

/// 已有非空值返回 `None`（保留原样），缺失才返回要写入的标签。
fn fill_gap<'a>(current: Option<&str>, label: &'a str) -> Option<&'a str> {
    match current {
        Some(value) if !value.trim().is_empty() => None,
        _ => Some(label),
    }
}

fn backup_path(database: &Path) -> PathBuf {
    let directory = database
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("backup");
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    directory.join(format!("tomatodesk-before-backfill-{stamp}.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        Options::parse(args.iter().map(|arg| (*arg).to_string()))
    }

    /// 默认必须是离线 dry-run：不带参数跑一次不该联网，也不该写库。
    #[test]
    fn defaults_to_offline_dry_run() {
        let options = parse(&[]).unwrap().unwrap();

        assert!(!options.apply);
        assert!(!options.fetch);
        assert!(!options.dev);
        assert!(options.limit.is_none());
    }

    #[test]
    fn parses_flags_and_limit() {
        let options = parse(&["--apply", "--fetch", "--dev", "--limit", "20"])
            .unwrap()
            .unwrap();

        assert!(options.apply && options.fetch && options.dev);
        assert_eq!(options.limit, Some(20));
    }

    #[test]
    fn rejects_conflicting_database_flags() {
        assert!(parse(&["--db", "a.sqlite3", "--dev"]).is_err());
    }

    #[test]
    fn rejects_bad_limit() {
        assert!(parse(&["--limit", "0"]).is_err());
        assert!(parse(&["--limit", "abc"]).is_err());
        assert!(parse(&["--limit"]).is_err());
    }

    #[test]
    fn help_skips_execution() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn blank_values_are_treated_as_missing() {
        assert_eq!(fill_gap(None, "x"), Some("x"));
        assert_eq!(fill_gap(Some("  "), "x"), Some("x"));
        assert_eq!(fill_gap(Some("已有"), "x"), None);
    }
}
