//! 横バー。8時間を全体とし、超えた日は総作業時間を全体とする

use windows_sys::Win32::Foundation::RECT;

use super::gdi::{rect, task_color, Painter};
use crate::record::{DaySummary, WORKDAY_SECONDS};

/// バー全体が表す秒数
fn span_seconds(summary: &DaySummary) -> i64 {
    summary.total().num_seconds().max(WORKDAY_SECONDS)
}

/// バーの中で `seconds` の位置にあたる x 座標
pub fn x_at(r: RECT, summary: &DaySummary, seconds: i64) -> i32 {
    let span = span_seconds(summary);
    let width = (r.right - r.left) as i64;
    r.left + (seconds.clamp(0, span) * width / span) as i32
}

pub fn draw(p: &Painter, r: RECT, summary: &DaySummary) {
    let radius = 4;
    let gap = p.px(1).max(1);
    let total = summary.total().num_seconds();
    let x_at = |seconds: i64| x_at(r, summary, seconds);

    p.fill_round(r, radius, p.theme.track);
    p.clip_round(r, radius, || {
        // 8時間に満たない分は「未稼働」として網掛け
        if total < WORKDAY_SECONDS {
            p.fill_hatch(rect(x_at(total), r.top, r.right, r.bottom), p.theme.hatch);
        }

        // 1時間ごとの目盛り
        for hour in 1..=(span_seconds(summary) - 1) / 3600 {
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
            p.fill(rect(x0, r.top, x1, r.bottom), task_color(task));
            // タスクの境目に隙間を入れる
            if i > 0 && x1 - x0 > gap * 2 {
                p.fill(rect(x0, r.top, x0 + gap, r.bottom), p.theme.bg);
            }
        }
    });

    // 8時間を超えたら 8時間の位置に点線
    if total > WORKDAY_SECONDS {
        let x = x_at(WORKDAY_SECONDS) - gap / 2;
        let (dash, space) = (p.px(3).max(2), p.px(2).max(1));
        let bottom = r.bottom + p.px(2);
        let mut y = r.top - p.px(2);
        while y < bottom {
            p.fill(rect(x, y, x + gap, (y + dash).min(bottom)), p.theme.text);
            y += dash + space;
        }
    }
}

/// 凡例に使う「未稼働」の印
pub fn draw_idle_marker(p: &Painter, r: RECT) {
    p.fill(r, p.theme.track);
    p.fill_hatch(r, p.theme.hatch);
}
