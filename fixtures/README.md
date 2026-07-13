# Fixtures (operator-supplied, not committed)

Everything in this directory except this file and `*.sha256` manifests is
gitignored. Nothing here is redistributable — see `../docs/LEGAL_PROVENANCE.md`.

## `v20-tests/` — CPU validation oracle

The V20 single-step tests (MIT, <https://github.com/SingleStepTests/v20>) live
in `v20-tests/*.json.gz`, flattened to `v20-tests/prepared/*.tsv` by
`tools/v20_prep.py`. They are MIT-licensed but large (~2.5 MB/opcode), so they
are fetched on demand, not committed. See `../docs/VALIDATION.md`. Fetch a
curated subset with `curl` from the repo's `v1_native/` directory; skip the
`0F*` files (V20 extensions the V30MZ lacks).

## Expected contents

```
fixtures/
  README.md              (committed)
  *.sha256               (committed manifests — hashes only, no ROM bytes)
  bios/
    ws-bios.rom          (your own dump; WonderSwan boot ROM)
    wsc-bios.rom         (your own dump; WonderSwan Color boot ROM)
  test-roms/             (cloned/built by tools/fetch-test-roms.ps1)
    ws-test-suite/...
    WSCpuTest.ws
    ...
  games/
    *.ws / *.wsc         (your own cartridge dumps)
  saves/                 (generated at runtime)
```

## Recording a fixture without shipping it

Tests verify a fixture by hash, not by contents. To register one:

```powershell
Get-FileHash -Algorithm SHA256 fixtures\test-roms\WSCpuTest.ws |
  ForEach-Object { "$($_.Hash.ToLower())  WSCpuTest.ws" } |
  Out-File -Encoding ascii fixtures\WSCpuTest.ws.sha256
```

The core and tests must fail with a clear "fixture not found" message when a
required file is missing — never fall back to an embedded copy.
