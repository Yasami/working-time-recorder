//! recorder が出力する記録ファイルの読み込みと集計

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const UNNAMED_TASK: &str = "(名称なし)";

/// 横バー全体が表す時間 (8時間)
pub const WORKDAY_SECONDS: i64 = 8 * 60 * 60;

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Start(String),
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub time: DateTime<Local>,
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskTotal {
    pub name: String,
    pub duration: Duration,
    /// ファイル内で最初に登場した順番。色分けに使う
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DaySummary {
    pub date: NaiveDate,
    /// その日に最初に登場した順
    pub tasks: Vec<TaskTotal>,
}

impl DaySummary {
    pub fn empty(date: NaiveDate) -> Self {
        DaySummary { date, tasks: Vec::new() }
    }

    pub fn total(&self) -> Duration {
        self.tasks
            .iter()
            .fold(Duration::zero(), |acc, t| acc + t.duration)
    }

    fn add(&mut self, name: &str, color: usize, duration: Duration) {
        match self.tasks.iter_mut().find(|t| t.name == name) {
            Some(task) => task.duration += duration,
            None => self.tasks.push(TaskTotal {
                name: name.to_string(),
                duration,
                color,
            }),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Records {
    events: Vec<Event>,
}

impl Records {
    pub fn parse(content: &str) -> Self {
        let mut events: Vec<Event> = content.lines().filter_map(parse_line).collect();
        events.sort_by_key(|e| e.time);
        Records { events }
    }

    /// 最後の記録が start なら、そのタスク名
    pub fn current_task(&self) -> Option<&str> {
        match &self.events.last()?.kind {
            EventKind::Start(name) => Some(name),
            EventKind::Stop => None,
        }
    }

    /// 日ごとのタスク別作業時間。
    /// start から次の記録 (start / stop) までをそのタスクの作業時間とし、
    /// 終了していない作業は `now` までとして数える。日をまたぐ作業は日付ごとに分割する。
    pub fn daily_summaries(&self, now: DateTime<Local>) -> BTreeMap<NaiveDate, DaySummary> {
        let mut colors: HashMap<&str, usize> = HashMap::new();
        let mut days: BTreeMap<NaiveDate, DaySummary> = BTreeMap::new();

        for (i, event) in self.events.iter().enumerate() {
            let EventKind::Start(name) = &event.kind else {
                continue;
            };
            let next_color = colors.len();
            let color = *colors.entry(name.as_str()).or_insert(next_color);

            let end = match self.events.get(i + 1) {
                Some(next) => next.time,
                None => now.max(event.time),
            };

            let mut start = event.time;
            while start < end {
                let date = start.date_naive();
                let mut segment_end = end;
                if let Some(next_day) = date.succ_opt() {
                    segment_end = segment_end.min(start_of_day(next_day));
                }
                if segment_end <= start {
                    segment_end = end;
                }
                days.entry(date)
                    .or_insert_with(|| DaySummary::empty(date))
                    .add(name, color, segment_end - start);
                start = segment_end;
            }
        }

        days
    }

    pub fn day_summary(&self, date: NaiveDate, now: DateTime<Local>) -> DaySummary {
        self.daily_summaries(now)
            .remove(&date)
            .unwrap_or_else(|| DaySummary::empty(date))
    }
}

fn parse_line(line: &str) -> Option<Event> {
    let mut columns = line.trim_end_matches('\r').splitn(3, '\t');
    let time = DateTime::parse_from_rfc3339(columns.next()?.trim())
        .ok()?
        .with_timezone(&Local);
    let kind = match columns.next()?.trim() {
        "start" => {
            let name = columns.next().unwrap_or("").trim();
            let name = if name.is_empty() { UNNAMED_TASK } else { name };
            EventKind::Start(name.to_string())
        }
        "stop" => EventKind::Stop,
        _ => return None,
    };
    Some(Event { time, kind })
}

fn start_of_day(date: NaiveDate) -> DateTime<Local> {
    let midnight = date.and_time(NaiveTime::MIN);
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .unwrap_or_else(|| Local.from_utc_datetime(&midnight))
}

/// recorder と同じ規則で記録ファイルのパスを決める
pub fn record_path() -> PathBuf {
    match env::var("WORKING_TIME_RECORD") {
        Ok(path) => PathBuf::from(path),
        Err(_) => dirs::home_dir()
            .unwrap_or_default()
            .join("working_time_record.txt"),
    }
}

pub fn load(path: &Path) -> Result<Records, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Records::parse(&String::from_utf8_lossy(&bytes))),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(format!(
            "記録ファイルが見つかりません: {}",
            path.display()
        )),
        Err(e) => Err(format!("記録ファイルを読み込めません: {}", e)),
    }
}

/// `h:mm` 形式
pub fn format_hm(duration: Duration) -> String {
    let minutes = duration.num_minutes().max(0);
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

pub fn weekday_ja(date: NaiveDate) -> &'static str {
    ["月", "火", "水", "木", "金", "土", "日"][date.weekday().num_days_from_monday() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(day: u32, hour: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, day, hour, min, 0).unwrap()
    }

    fn start(time: DateTime<Local>, task: &str) -> String {
        format!("{}\tstart\t{}\n", time.to_rfc3339(), task)
    }

    fn stop(time: DateTime<Local>) -> String {
        format!("{}\tstop\t\n", time.to_rfc3339())
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    fn durations(summary: &DaySummary) -> Vec<(&str, i64)> {
        summary
            .tasks
            .iter()
            .map(|t| (t.name.as_str(), t.duration.num_minutes()))
            .collect()
    }

    #[test]
    fn test_parse_skips_invalid_lines() {
        let content = format!(
            "{}invalid line\n\n{}\tunknown\tx\n{}",
            start(at(14, 9, 0), "a"),
            at(14, 9, 30).to_rfc3339(),
            stop(at(14, 10, 0))
        );
        let records = Records::parse(&content);
        assert_eq!(records.events.len(), 2);
    }

    #[test]
    fn test_aggregates_by_task_name() {
        let content = [
            start(at(14, 9, 0), "設計"),
            start(at(14, 10, 30), "レビュー"),
            stop(at(14, 11, 0)),
            start(at(14, 13, 0), "設計"),
            stop(at(14, 14, 15)),
        ]
        .concat();
        let summary = Records::parse(&content).day_summary(date(14), at(14, 18, 0));
        assert_eq!(durations(&summary), vec![("設計", 165), ("レビュー", 30)]);
        assert_eq!(summary.total().num_minutes(), 195);
    }

    #[test]
    fn test_running_task_counts_until_now() {
        let content = start(at(14, 9, 0), "a");
        let records = Records::parse(&content);
        let summary = records.day_summary(date(14), at(14, 9, 45));
        assert_eq!(durations(&summary), vec![("a", 45)]);
        assert_eq!(records.current_task(), Some("a"));
    }

    #[test]
    fn test_split_at_midnight() {
        let content = [start(at(14, 23, 0), "a"), stop(at(15, 1, 30))].concat();
        let days = Records::parse(&content).daily_summaries(at(15, 12, 0));
        assert_eq!(durations(&days[&date(14)]), vec![("a", 60)]);
        assert_eq!(durations(&days[&date(15)]), vec![("a", 90)]);
    }

    #[test]
    fn test_lines_are_sorted_and_colors_follow_first_appearance() {
        let content = [
            start(at(15, 9, 0), "b"),
            stop(at(15, 10, 0)),
            start(at(14, 9, 0), "a"),
            start(at(14, 10, 0), "b"),
            stop(at(14, 11, 0)),
        ]
        .concat();
        let records = Records::parse(&content);
        let days = records.daily_summaries(at(15, 12, 0));
        let colors: Vec<(&str, usize)> = days[&date(15)]
            .tasks
            .iter()
            .map(|t| (t.name.as_str(), t.color))
            .collect();
        assert_eq!(colors, vec![("b", 1)]);
        assert_eq!(records.current_task(), None);
    }

    #[test]
    fn test_empty_task_name() {
        let content = [start(at(14, 9, 0), ""), stop(at(14, 9, 10))].concat();
        let summary = Records::parse(&content).day_summary(date(14), at(14, 12, 0));
        assert_eq!(durations(&summary), vec![(UNNAMED_TASK, 10)]);
    }

    #[test]
    fn test_crlf_line_endings() {
        let content = format!("{}\tstart\ta\r\n{}\tstop\t\r\n", at(14, 9, 0).to_rfc3339(), at(14, 9, 20).to_rfc3339());
        let summary = Records::parse(&content).day_summary(date(14), at(14, 12, 0));
        assert_eq!(durations(&summary), vec![("a", 20)]);
    }

    #[test]
    fn test_format_hm() {
        assert_eq!(format_hm(Duration::minutes(0)), "0:00");
        assert_eq!(format_hm(Duration::minutes(65)), "1:05");
        assert_eq!(format_hm(Duration::minutes(600)), "10:00");
    }
}
