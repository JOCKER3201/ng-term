fn draw() {
    let power = if host.battery == () {
        "AC"
    } else if host.charging {
        "" + host.battery + "% +"
    } else {
        "" + host.battery + "%"
    };
    [columns([
        ["DATE",   upper(host.date)],
        ["UPTIME", uptime(host.uptime)],
        ["POWER",  power],
    ])]
}
