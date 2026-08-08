fn draw() {
    let footer = [["LOAD", round(host.load1, 2) + "  " + round(host.load5, 2)
                          + "  " + round(host.load15, 2)]];
    // A machine with no temperature sensor says nothing about heat.
    if host.temp != () {
        footer[0][1] = footer[0][1] + "    " + round(host.temp, 0) + "°C";
    }
    [
        title("CPU", upper(host.cpu_name)),
        gauges(host.cpu_each, 2),
        rows(footer),
    ]
}
