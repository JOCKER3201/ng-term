<p align="center">
  <img src="assets/ng-term.png" alt="ng-term logo" width="440">
</p>

# ng-term

> **THIS PROJECT IS NOT A CLONE OF eDEX-UI. IT IS AN INDEPENDENT PROJECT INSPIRED BY eDEX-UI.**
>
> **THIS PROJECT WAS WRITTEN ENTIRELY BY THE FABLE 5 AI MODEL BY ANTHROPIC.**
>
> **THE LOGO WAS ALSO AI-GENERATED, USING CANVA.**

## Widgets

Widgets live in `~/.local/share/ng-term/widgets`, one directory each.
A widget is either a **Rhai script** (`<name>.rhai`) or a **compiled
plugin** (`<name>.so`), alongside whatever assets it needs.

Scripts are the ordinary way to write one. They are sandboxed by
construction — a script sees the host data and the drawing vocabulary
and nothing else, so it cannot read a file, open a socket or start a
process — they survive upgrades untouched, and one script works on every
platform.

Compiled plugins exist for the few widgets a script cannot express: the
terminal view, which draws thousands of character cells per frame, and
the file browser, which has to read directories. They are the escape
hatch, not the default.

> ### This warning is about `.so` plugins only
>
> **None of it applies to `.rhai` scripts.** A script cannot reach the
> filesystem, the network or another process, because no function that
> would let it exists in its world. Installing a script risks nothing
> but a badly drawn panel.
>
> **A compiled plugin is the opposite: native code running with your
> full account privileges, in a program that sits next to your shell.**
> There is no sandbox around it, and none is possible. Installing one is
> the same act of trust as building a package from the AUR or adding a
> third-party repository — judge the author, not the mechanism.
>
> A plugin must also be rebuilt for each release, and separately for
> each platform and processor architecture; a script is written once and
> works everywhere, on every version. Plugins shipped with ng-term are
> overwritten on every install, so an outdated one cannot linger;
> plugins you add yourself are left alone.
>
> Prefer a script. Reach for a plugin only when a script genuinely
> cannot do the job.

`NGTERM_SAFE=1` starts the program with every plugin skipped, which is
the way back in when one of them prevents startup.

## Installation

Requirements: Linux, a Vulkan driver, Rust (cargo), GNU make.

```sh
make install        # clean build + install to ~/.local/
sudo make install   # clean build + install to /usr/local/
```

Then run:

```sh
ng-term
```

Uninstall:

```sh
make uninstall        # if installed to ~/.local/
sudo make uninstall   # if installed to /usr/local/
```
