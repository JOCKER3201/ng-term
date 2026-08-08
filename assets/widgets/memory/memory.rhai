fn draw() {
    let out = [
        title("MEMORY"),
        dots(host.mem_fraction),
        rows([["", bytes(host.mem_used) + " / " + bytes(host.mem_total)]]),
    ];
    // No swap configured means no swap meter, rather than an empty bar.
    if host.swap_total > 0.0 {
        out.push(meter("SWAP", host.swap_fraction, bytes(host.swap_used)));
    }
    out
}
