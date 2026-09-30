# TOOLBUG-033 — a UTF-8 BOM makes a Modelica file unparseable, and takes its whole library with it

**Status:** fixed (2026-09-30). `parse_to_syntax` turns a leading U+FEFF into three spaces, keeping every byte offset; test `a_leading_byte_order_mark_is_not_source_text`.
**Severity:** high — one byte in one file zeroes a library's coverage.

## What

A `.mo` file beginning with the UTF-8 byte-order mark `EF BB BF` fails to
parse: the lexer rejects the BOM itself as an unexpected character.

```
$ printf '\xEF\xBB\xBFmodel M\n  Real x;\nequation\n  x = 1;\nend M;\n' > a/M.mo
$ rumoca compile a/M.mo --model M
  [EP001] unexpected `﻿`
   ╭─[a/M.mo:1:1]
 1 │ ﻿model M
   · ┬
   · ╰── unexpected `﻿`
  help: expected one of: end of input, block, class, connector, encapsulated,
        expandable, final, function, impure, model, operator, package,
        partial, pure, record, type, within
```

The identical file without the three leading bytes compiles. The real-world
instance that surfaced this is
`Modelica_DeviceDrivers/Blocks/Communication.mo`:

```
$ head -c 8 Modelica_DeviceDrivers/Blocks/Communication.mo | xxd
00000000: efbb bf77 6974 6869                      ...withi
```

Strip the BOM and that file parses too, reaching a real diagnostic about the
library (`PKG-006`, a directory with no `package.mo`).

## Why it is worse than one file

`--source-root` parses the whole tree before compiling anything, so a single
BOM file makes *every* model in that library fail, each reporting
`E004 parse error: <root>: parse source-root files under <root>`. The
library scores zero, and nothing in the diagnostic points at the file.

Measured over the evaluation dataset:

| Library | BOM files | .mo files | compile targets blocked |
|---|---:|---:|---:|
| TRANSFORM | 25 | 3269 | 1238 |
| IDEAS | 18 | 3717 | 2062 |
| Modelica_DeviceDrivers | 5 | 212 | 165 |
| AixLib | 2 | 4571 | 2411 |
| **total** | **50** | | **5876** |

Four libraries of eleven, and **5876 of 13028 targets — 45% of the dataset —
blocked by 50 bytes of BOM.** The other seven libraries contain none, which
is why this had not been seen before: MSL and the LBNL/IBPSA family are
BOM-free, and a corpus drawn from them looks clean.

## Why a BOM is legitimate input

A BOM is valid UTF-8 and Windows editors write it by default. The Modelica
specification says nothing that forbids it, and OpenModelica and Dymola both
accept these same files — the libraries above are shipped, used and
simulated daily. A tool that rejects them is the outlier.

## Suggested fix

Skip a leading `EF BB BF` when reading a source file, in the one place
source text is read, so every entry point inherits it. A parser-level fix
would be narrower but would leave the same trap for any other reader.

## Reproduction

```bash
# An empty directory of its own: `compile` also reads sibling .mo files, so
# a shared scratch directory makes the run fail on a duplicate class instead
# and looks like a reproduction when it is not.
mkdir -p /tmp/bom && cd /tmp/bom
printf '\xEF\xBB\xBFmodel M\n  Real x;\nequation\n  x = 1;\nend M;\n' > M.mo
rumoca compile M.mo --model M          # EP001 unexpected `﻿` at 1:1
sed -i '1s/^\xEF\xBB\xBF//' M.mo
rumoca compile M.mo --model M          # compiles
```

Verified against `rumoca 0.10.0` (release build) on 2026-09-27.

## Related

[TOOLBUG-034](TOOLBUG-034-source-root-parse-errors-do-not-name-the-file.md) —
why finding the offending file took a bisection over 212 files.
