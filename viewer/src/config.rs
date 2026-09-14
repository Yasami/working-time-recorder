//! 記録ファイルの横に置く設定ファイル (JSON)

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::record::{parse_color, Label, Labels};

/// 前回の変化からパネルを表示するまでの既定の時間 (分)
const DEFAULT_POPUP_INTERVAL_MINUTES: f64 = 30.0;
/// 自動で表示したパネルを消すまでの既定の時間 (秒)
const DEFAULT_AUTO_HIDE_SECONDS: f64 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// 記録ファイルが前回変化してから、パネルを自動で表示するまでの時間。None なら表示しない
    pub popup_interval: Option<Duration>,
    /// 自動で表示したパネルを消すまでの時間。None なら消さない
    pub auto_hide: Option<Duration>,
    pub labels: Labels,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            popup_interval: positive_duration(DEFAULT_POPUP_INTERVAL_MINUTES * 60.0),
            auto_hide: positive_duration(DEFAULT_AUTO_HIDE_SECONDS),
            labels: Labels::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    popup_interval_minutes: Option<f64>,
    auto_hide_seconds: Option<f64>,
    #[serde(default)]
    labels: HashMap<String, RawLabel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLabel {
    label: Option<String>,
    color: Option<String>,
}

/// 0 以下は「無効」
fn positive_duration(seconds: f64) -> Option<Duration> {
    (seconds.is_finite() && seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
}

pub fn parse(content: &str) -> Result<Config, String> {
    let raw: RawConfig = serde_json::from_str(content.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("設定ファイルの形式が正しくありません: {}", e))?;

    let mut labels = Labels::new();
    for (task, label) in raw.labels {
        let color = match label.color.as_deref() {
            Some(code) => Some(parse_color(code).ok_or_else(|| {
                format!("設定ファイルのカラーコードが正しくありません: {} ({})", code, task)
            })?),
            None => None,
        };
        let text = label.label.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        labels.insert(task, Label { text, color });
    }

    Ok(Config {
        popup_interval: positive_duration(
            raw.popup_interval_minutes.unwrap_or(DEFAULT_POPUP_INTERVAL_MINUTES) * 60.0,
        ),
        auto_hide: positive_duration(raw.auto_hide_seconds.unwrap_or(DEFAULT_AUTO_HIDE_SECONDS)),
        labels,
    })
}

/// 記録ファイル名の拡張子 `.txt` を `.config` に替えたパス。
/// 拡張子が `.txt` でなければ、ファイル名の末尾に `.config` を足す
pub fn config_path(record_path: &Path) -> PathBuf {
    let is_txt = record_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
    let base = if is_txt { record_path.with_extension("") } else { record_path.to_path_buf() };
    let mut path = base.into_os_string();
    path.push(".config");
    PathBuf::from(path)
}

/// 設定ファイルは無くてもよい (既定値を使う)
pub fn load(path: &Path) -> Result<Config, String> {
    match fs::read(path) {
        Ok(bytes) => parse(&String::from_utf8_lossy(&bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("設定ファイルを読み込めません: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let config = parse(
            r##"{
                "popup_interval_minutes": 30,
                "auto_hide_seconds": 5.5,
                "labels": {
                    "a": { "label": "設計作業", "color": "#FF8800" },
                    "b": { "color": "navy" },
                    "c": { "label": "レビュー" },
                    "d": { "label": " " }
                }
            }"##,
        )
        .unwrap();
        assert_eq!(config.popup_interval, Some(Duration::from_secs(30 * 60)));
        assert_eq!(config.auto_hide, Some(Duration::from_millis(5500)));
        assert_eq!(config.labels.len(), 4);
        assert_eq!(config.labels["a"], Label { text: Some("設計作業".into()), color: Some(0xFF8800) });
        assert_eq!(config.labels["b"], Label { text: None, color: Some(0x000080) });
        assert_eq!(config.labels["c"], Label { text: Some("レビュー".into()), color: None });
        assert_eq!(config.labels["d"], Label::default());
    }

    #[test]
    fn test_parse_defaults() {
        assert_eq!(parse("\u{feff}{}").unwrap(), Config::default());
    }

    #[test]
    fn test_parse_zero_disables_timers() {
        let config = parse(r#"{ "popup_interval_minutes": 0, "auto_hide_seconds": -1 }"#).unwrap();
        assert_eq!(config.popup_interval, None);
        assert_eq!(config.auto_hide, None);
    }

    #[test]
    fn test_parse_errors() {
        assert!(parse("").is_err());
        assert!(parse(r#"{ "auto_hide_seconds": "10" }"#).is_err());
        assert!(parse(r#"{ "unknown": 1 }"#).is_err());
        assert!(parse(r#"{ "labels": { "a": { "color": "invalid" } } }"#).is_err());
    }

    #[test]
    fn test_config_path() {
        assert_eq!(
            config_path(Path::new("dir/working_time_record.txt")),
            PathBuf::from("dir/working_time_record.config")
        );
        assert_eq!(config_path(Path::new("dir/record.TXT")), PathBuf::from("dir/record.config"));
        assert_eq!(config_path(Path::new("dir/record.log")), PathBuf::from("dir/record.log.config"));
        assert_eq!(config_path(Path::new("dir/record")), PathBuf::from("dir/record.config"));
    }
}
