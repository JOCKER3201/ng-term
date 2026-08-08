fn draw() {
    let body = [];
    for p in host.processes {
        body.push(["" + p.pid, p.name, round(p.cpu, 1) + "%", round(p.mem, 1) + "%"]);
    }
    [table(
        [["PID", "right"], ["NAME", "left"], ["CPU", "right"], ["MEM", "right"]],
        body,
        1,
    )]
}
