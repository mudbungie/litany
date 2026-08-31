+++
title = "the macOS artifact comes off the same container line: litany and bz cross-produced for aarch64-apple-darwin"
created = 1788138707
updated = 1788138708
claimant = "OrderMac"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
The macOS half of the containerized-build ruling (yog bl-888d, under yog DESIGN
Section 10 and 10.1), landed in thrall first (thrall bl-e479) and portable here for
the price of one dependency feature term.

WHAT LANDS: a build-only `Containerfile.mac` and a `make mac-artifact` target
that cross-produce BOTH binaries an install needs — `litany` and the pinned
`bz` — for `aarch64-apple-darwin` from a Linux container, plus a verifier that
reads the produced Mach-O rather than trusting the build's exit code, plus the
README section that says what is proven and what is not. Both binaries, because
the image already refuses to ship the engine without the adapter: an install
route that cannot answer a prompt is not an install route.

THE TOOLCHAIN IS zig cc (cargo-zigbuild), and osxcross is refused on Apple's
own terms rather than on taste. The Xcode and Apple SDKs Agreement section 2.7 forbids
uploading, hosting, redistributing or sublicensing the Apple software, so the
SDK may never sit in this tree or in anything published from it; section 2.5 forbids
separately using the Apple SDKs or running any part of the Apple software on
non-Apple-branded hardware, which kills the usual escape of taking an SDK path
as a build argument on a Linux builder. There is no arm's-length version of
that arrangement to build. zig ships one darwin stub of its own
(`lib/libc/darwin/libSystem.tbd`) under its own licence and acquires nothing
from Apple.

THE ONE BLOCKER, AND IT IS A SUBTRACTION. zig ships libSystem and NO framework
stubs, so a crate graph that links any Apple framework cannot cross here. This
graph had exactly one such edge: `chrono`'s `clock` feature pulls
`iana-time-zone`, which links CoreFoundation on darwin. `clock` is `now` plus
timezone detection, and this crate uses `Utc` only — `src/prompt/clock.rs` is
the whole of the use and it names `DateTime` and `Utc`. Narrowing the feature
to `now` drops FIVE crates from the lockfile (`iana-time-zone`,
`iana-time-zone-haiku`, `core-foundation-sys`, `android_system_properties`,
`windows-link`) and nothing else changes. A smaller graph and a portable one by
the same edit.

EVIDENCE IS READ, NOT ASSERTED. No mac exists on the build box, so nothing is
executed. `scripts/mac-verify.sh` reads the Mach-O header, architecture,
filetype, LC_BUILD_VERSION and every LC_LOAD_DYLIB out of each produced file
and requires every library to be a stock macOS path, and it runs its negative
direction first — malformed inputs it must refuse — because a checker that has
stopped checking passes everything forever. Record what that leaves proven and
what it does not.

The build image is a fixture and is never pushed, so `make image-scan` does not
apply to it; the artifacts are compiled from the tree the source gate already
reads, exactly as the Linux release binaries are.

Gates: 100% coverage and the whole suite green (the feature narrowing touches
the build, so the suite is the check that it touched nothing else); the README
updated; alignment against the spec docs.