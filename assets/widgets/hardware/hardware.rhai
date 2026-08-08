// ng-term widget. This file IS the widget: called once a frame, it
// returns the elements to draw. Any assets it needs live beside it.
//
// Everything readable is in `host`; everything doable is the element and
// formatting functions. There is no way to reach the filesystem, the
// network or another process, because no such function exists here.

fn draw() {
    [
        title("HARDWARE"),
        rows([
            ["MANUFACTURER", upper(host.manufacturer)],
            ["MODEL",        upper(host.model)],
            ["CHASSIS",      host.chassis],
        ]),
    ]
}
