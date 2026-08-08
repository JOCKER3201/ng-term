// The colon blinks off for half of each second; `host.t` is the
// animation clock, so the phase comes from the frame, not from a timer.
fn draw() {
    let sep = if host.t - floor(host.t) < 0.5 { " " } else { ":" };
    let hh = if host.hour < 10 { "0" } else { "" } + host.hour;
    let mm = if host.minute < 10 { "0" } else { "" } + host.minute;
    let ss = if host.second < 10 { "0" } else { "" } + host.second;
    [
        text(hh + sep + mm + sep + ss, "center", 2.4),
        text(upper(host.date_long), "center", 0.85),
    ]
}
