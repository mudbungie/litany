#!/usr/bin/env bash
# mac-verify — read the macOS artifact and say what it IS.
#
#   scripts/mac-verify.sh PATH        verify one produced binary
#   scripts/mac-verify.sh --self-test verify the verifier, then nothing else
#
# PORTED FROM THRALL (its bl-e479), under yog `docs/DESIGN.md` §10. The copy is
# deliberately close to byte-identical, for the same reason `image-scan.sh`
# there is a close copy of this repo's: a short `diff` against the sibling is
# the only cheap defence against two copies drifting. §10.1 is explicit that
# each repo owns its own build tooling and that there is no shared build system
# — the components meet at the wire and nowhere else — so a copy is the shape,
# not an oversight.
#
# WHY THIS EXISTS AT ALL. The artifact is cross-produced on a Linux box that
# has no mac to run it on (README "The macOS artifact"). "The build succeeded"
# is therefore the whole of what the build can tell us, and it is not enough: a
# linker that quietly produced the wrong architecture, a graph that acquired a
# dependency on a dylib no stock mac carries, or a binary macOS would refuse to
# start all look identical to a green build. Every one of those is READABLE out
# of the file itself, on any platform, with no Apple tooling — this reads them.
#
# WHAT IT PROVES, and it is worth being exact about the boundary:
#
#   proven    the file is a 64-bit Mach-O executable for arm64; it targets the
#             macOS platform; the minimum OS it declares; every dynamic library
#             it will ask macOS for at load time; and that it carries a code
#             signature at all.
#   NOT proven  that it runs. No mac executes it here. An artifact that passes
#             this has the shape of a working mac binary and has not been
#             observed to be one.
#
# The dylib list is the check that earns its keep. A Mach-O binary names the
# libraries it loads by absolute install path, so "does this need anything the
# operator must install first" is answerable from the bytes: every entry under
# `/usr/lib/` or `/System/Library/` ships with macOS, and anything else is a
# dependency the artifact acquired without anyone deciding to.
#
# BOTH DIRECTIONS, the discipline `leak-scan` and `rules-audit` already hold in
# this repo. `--self-test` feeds the reader files it MUST refuse — a truncated
# one, a non-Mach-O one, a Mach-O for the wrong architecture, one that is not
# an executable, and one whose load commands enumerate nothing — because a
# checker that has stopped checking passes everything forever. The positive
# direction is the real artifact, which `make mac-artifact` runs immediately
# after this.
#
# BASH 3.2 AND `od`/`dd` ONLY. macOS ships bash 3.2 and always will, so there
# are no associative arrays and no `mapfile` here; and the reader must work on
# a box with no Rust, no python and no llvm — a `Makefile`, `od` and `dd`. Both
# platforms this runs on are little-endian, which is what lets `od -tu4` read a
# Mach-O 64-bit little-endian header without byte-swapping; the magic check
# below is what catches a host where that stopped being true (a big-endian host
# reads 0xfeedfacf as 0xcffaedfe and is refused as "not a 64-bit Mach-O"),
# rather than silently misreading every field after it.

set -eu

MAGIC_MH_MAGIC_64=4277009103   # 0xfeedfacf, 64-bit Mach-O, little-endian
CPU_TYPE_ARM64=16777228        # 0x0100000c
MH_EXECUTE=2                   # filetype: an executable, not a dylib or object
PLATFORM_MACOS=1               # LC_BUILD_VERSION platform
LC_LOAD_DYLIB=12               # 0x0c
LC_BUILD_VERSION=50            # 0x32
LC_CODE_SIGNATURE=29           # 0x1d

say()  { echo "mac-verify: $*"; }
fail() { echo "mac-verify: $*" >&2; exit 1; }

# A little-endian u32 at a byte offset. `od` reads in host order; the magic
# check above is what makes that assumption safe rather than silent.
u32() {
  od -An -tu4 -j "$2" -N 4 "$1" 2>/dev/null | tr -d ' \n'
}

# A NUL-terminated string of at most $3 bytes at offset $2.
cstr() {
  dd if="$1" bs=1 skip="$2" count="$3" 2>/dev/null | tr -d '\000'
}

# A packed Mach-O version: 0xXXXXYYZZ is X.Y.Z.
version() {
  echo "$(( $1 >> 16 )).$(( ($1 >> 8) & 255 )).$(( $1 & 255 ))"
}

verify() {
  bin="$1"

  [ -f "$bin" ] || fail "$bin: no such file"

  size=$(wc -c < "$bin" | tr -d ' ')
  [ "$size" -ge 32 ] || fail "$bin: $size bytes — too short to hold a Mach-O header"

  magic=$(u32 "$bin" 0)
  [ "$magic" = "$MAGIC_MH_MAGIC_64" ] ||
    fail "$bin: not a 64-bit little-endian Mach-O (magic $magic)"

  cputype=$(u32 "$bin" 4)
  [ "$cputype" = "$CPU_TYPE_ARM64" ] ||
    fail "$bin: cputype $cputype, expected arm64 ($CPU_TYPE_ARM64)"

  filetype=$(u32 "$bin" 12)
  [ "$filetype" = "$MH_EXECUTE" ] ||
    fail "$bin: filetype $filetype, expected an executable ($MH_EXECUTE)"

  ncmds=$(u32 "$bin" 16)
  [ "$ncmds" -gt 0 ] ||
    fail "$bin: no load commands — nothing to read, so nothing is proven"

  say "$bin"
  say "  Mach-O 64-bit executable, arm64, $size bytes"

  # Walk the load commands. Each is a (cmd, size) pair followed by its body;
  # a zero size would spin forever, so it is refused rather than skipped.
  platform=""
  minos=""
  sdk=""
  signed=no
  dylibs=""
  ndylibs=0
  off=32
  i=0
  while [ "$i" -lt "$ncmds" ]; do
    cmd=$(u32 "$bin" "$off")
    cmdsize=$(u32 "$bin" $(( off + 4 )))
    [ -n "$cmd" ] && [ -n "$cmdsize" ] ||
      fail "$bin: load command $i runs past the end of the file"
    [ "$cmdsize" -gt 0 ] ||
      fail "$bin: load command $i declares size 0"

    if [ "$cmd" = "$LC_BUILD_VERSION" ]; then
      platform=$(u32 "$bin" $(( off + 8 )))
      minos=$(version "$(u32 "$bin" $(( off + 12 )))")
      sdk=$(version "$(u32 "$bin" $(( off + 16 )))")
    elif [ "$cmd" = "$LC_CODE_SIGNATURE" ]; then
      signed=yes
    elif [ "$cmd" = "$LC_LOAD_DYLIB" ]; then
      nameoff=$(u32 "$bin" $(( off + 8 )))
      name=$(cstr "$bin" $(( off + nameoff )) $(( cmdsize - nameoff )))
      [ -n "$name" ] || fail "$bin: a load command names an empty library"
      dylibs="$dylibs $name"
      ndylibs=$(( ndylibs + 1 ))
    fi

    off=$(( off + cmdsize ))
    i=$(( i + 1 ))
  done

  [ "$platform" = "$PLATFORM_MACOS" ] ||
    fail "$bin: LC_BUILD_VERSION platform '${platform:-absent}', expected macOS ($PLATFORM_MACOS)"
  say "  platform macOS, minimum OS $minos, built against SDK $sdk"

  # An arm64 mac refuses to start an unsigned binary outright. The signature
  # the cross-linker applies is ad-hoc — it satisfies that rule and it is NOT
  # notarization; a downloaded copy still carries a quarantine attribute that
  # only the operator, on a mac, can clear or replace with a real signature.
  [ "$signed" = yes ] ||
    fail "$bin: no LC_CODE_SIGNATURE — an arm64 mac will refuse to start it"
  say "  code signature present (ad-hoc; not notarized)"

  [ "$ndylibs" -gt 0 ] ||
    fail "$bin: no LC_LOAD_DYLIB at all — the reader found nothing, which is not a clean bill"

  for d in $dylibs; do
    case "$d" in
      /usr/lib/*|/System/Library/*) say "  loads $d (ships with macOS)" ;;
      *) fail "$bin: loads $d — not a stock macOS path; the artifact is not self-contained" ;;
    esac
  done

  say "  $ndylibs dynamic libraries, all stock. VERIFIED — shape only, not executed."
}

# --- the negative direction --------------------------------------------------
#
# Five fabricated inputs, each malformed in one way this reader must catch. A
# header is 32 bytes of little-endian u32s: magic, cputype, cpusubtype,
# filetype, ncmds, sizeofcmds, flags, reserved.

refuses() {
  what="$1"
  file="$2"
  if ( verify "$file" ) >/dev/null 2>&1; then
    fail "self-test: $what was ACCEPTED — the reader has stopped reading"
  fi
  say "self-test: refused $what"
}

self_test() {
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/mac-verify.XXXXXXXX")
  trap 'rm -rf "$scratch"' EXIT

  printf '\xcf\xfa\xed' > "$scratch/truncated"
  refuses "a truncated file" "$scratch/truncated"

  printf '\x7fELF\x02\x01\x01\x00' > "$scratch/elf"
  printf '\x00\x00\x00\x00\x00\x00\x00\x00' >> "$scratch/elf"
  printf '\x02\x00\xb7\x00\x01\x00\x00\x00' >> "$scratch/elf"
  printf '\x00\x00\x00\x00\x00\x00\x00\x00' >> "$scratch/elf"
  refuses "a file that is not Mach-O at all" "$scratch/elf"

  # magic, cputype x86_64 (0x01000007), cpusubtype, MH_EXECUTE, 1 command.
  printf '\xcf\xfa\xed\xfe\x07\x00\x00\x01\x03\x00\x00\x00\x02\x00\x00\x00' \
    > "$scratch/wrongarch"
  printf '\x01\x00\x00\x00\x10\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' \
    >> "$scratch/wrongarch"
  refuses "a Mach-O for the wrong architecture" "$scratch/wrongarch"

  # arm64, but filetype MH_DYLIB (6) rather than an executable.
  printf '\xcf\xfa\xed\xfe\x0c\x00\x00\x01\x00\x00\x00\x00\x06\x00\x00\x00' \
    > "$scratch/notexec"
  printf '\x01\x00\x00\x00\x10\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' \
    >> "$scratch/notexec"
  refuses "a Mach-O that is not an executable" "$scratch/notexec"

  # arm64 executable declaring zero load commands: nothing to read, so nothing
  # is proven, and an empty enumeration must never pass as a clean bill.
  printf '\xcf\xfa\xed\xfe\x0c\x00\x00\x01\x00\x00\x00\x00\x02\x00\x00\x00' \
    > "$scratch/nocmds"
  printf '\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' \
    >> "$scratch/nocmds"
  refuses "an executable whose load commands enumerate nothing" "$scratch/nocmds"

  say "self-test: 5 malformed inputs, all refused"
}

case "${1:-}" in
  --self-test) self_test ;;
  "")          fail "usage: mac-verify.sh PATH | --self-test" ;;
  *)           verify "$1" ;;
esac
