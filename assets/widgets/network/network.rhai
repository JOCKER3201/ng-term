fn draw() {
    [
        title("NETWORK", upper(host.iface)),
        rows([
            ["STATE", if host.online { "ONLINE" } else { "OFFLINE" }],
            ["IPV4",  if host.ipv4 == () { "--" } else { host.ipv4 }],
            ["PING",  if host.ping == () { "--" } else { "" + host.ping + "ms" }],
        ]),
    ]
}
