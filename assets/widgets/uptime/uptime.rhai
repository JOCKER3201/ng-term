// A widget that exists only as a script — there is no compiled code
// behind it. It also shows what a script can do that a static
// description cannot: decide what to show.

fn draw() {
    let lines = [
        ["UP",   uptime(host.uptime)],
        ["HOST", upper(host.name)],
        ["USER", upper(host.user)],
    ];
    // Only worth a line when there is something to put in it.
    if host.kernel != "" {
        lines.push(["KERNEL", host.kernel]);
    }
    // Machines without a battery say nothing rather than "--".
    let power = if host.charging == () {
        ""
    } else if host.charging {
        "CHARGING"
    } else {
        "BATTERY"
    };
    [
        title("UPTIME", power),
        rows(lines),
    ]
}
