# Legal & Provenance Policy

This repository contains **only** original source code. It never contains, and
must never contain, any of the following:

- The WonderSwan / WonderSwan Color BIOS (boot ROM). It was dumped in 2019 and
  remains Bandai's copyrighted firmware. The operator supplies their own dump.
- Cartridge dumps of commercial games (`.ws`, `.wsc`, `.pc2`). Operator-owned,
  operator-supplied.
- Save data, EEPROM images, or save-states derived from the above.
- Hardware test-ROM **binaries**. Their source is open, but the built `.ws`
  files are treated like any other ROM and kept out of the repo.
- Any operator filesystem path, machine name, or personal identifier — not in
  code, comments, commit messages, or logs.

## How fixtures are supplied

Everything non-redistributable lives under `fixtures/`, which is gitignored
except for `README.md` and `*.sha256` manifests. See `fixtures/README.md` for the
expected filenames and how to record a SHA-256 so tests can verify a fixture
without shipping it.

The CLI and tests must fail cleanly with a "fixture not found" style message when
a required fixture is absent — never embed a fallback copy.

## Test-ROM licensing

The hardware test ROMs (ws-test-suite, WSCpuTest, WSTimingTest, WSHWTest,
rtctest, ARMV30MZ reference) are open source under their authors' licenses. We
consume them as **oracles**, cite them, and record each one's license and source
commit in `docs/TEST_ROMS.md`. We do not vendor their binaries.

## Reference emulators

ares and Mednafen are used only as external behavioural **oracles** (run
separately by the operator to produce reference framebuffer/audio captures for
diffing). No code is copied from them; observed behaviour is re-derived and
cited. Before adopting any code or contributing upstream, check that project's
contribution and AI-authorship policy first.

## Project license

The source is PolyForm Noncommercial 1.0.0 (declared in the workspace
`Cargo.toml`). The WonderSwan name and world are the property of their owners.
