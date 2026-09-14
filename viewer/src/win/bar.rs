//! 8時間を全体とした横バー

use windows_sys::Win32::Foundation::RECT;

use super::gdi::{rect, task_color, Painter};
use crate::record::{DaySummary, WORKDAY_SECONDS};

pub fn draw(p: &Painter, r: RECT, summary: &DaySummary) {
    let radius = 4;
    let width = (r.right - r.left) as i64;
    let gap = p.px(1).max(1);
    let x_at = |seconds: i64| r.left + (seconds.clamp(0, WORKDAY_SECONDS) * width / WORKDAY_SECONDS) as i32;

    p.fill_round(r, radius, p.theme.track);
    p.clip_round(r, radius, || {
        // 1時間ごとの目盛り
        for hour in 1..8 {
            let x = x_at(hour * 3600);
            p.fill(rect(x, r.top, x + gap, r.bottom), p.theme.tick);
        }

        let mut elapsed = 0;
        for (i, task) in summary.tasks.iter().enumerate() {
            let x0 = x_at(elapsed);
            elapsed += task.duration.num_seconds();
            let x1 = x_at(elapsed);
            if x1 <= x0 {
                continue;
            }
            p.fill(rect(x0, r.top, x1, r.bottom), task_color(task.color));
            // タスクの境目に隙間を入れる
            if i > 0 && x1 - x0 > gap * 2 {
                p.fill(rect(x0, r.top, x0 + gap, r.bottom), p.theme.bg);
            }
        }
    });
}
