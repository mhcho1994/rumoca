# TOOLBUG-034 — a source-root parse error names the root, not the file

**Status:** open. Found while diagnosing
[TOOLBUG-033](TOOLBUG-033-a-utf8-bom-makes-a-file-unparseable.md).
**Severity:** medium — correct verdict, unusable message.

## What

When `--source-root` walks a tree and one file fails to parse, the error
identifies the directory it was given:

```
rumoca::compiler::E004
  parse error: /path/to/repo: parse source-root files under /path/to/repo
```

The compiler knows which file failed — pointed at that file directly it
produces a precise span, the offending token, and a list of what it
expected. None of that survives the source-root path. The message names the
argument the user typed, which they already knew.

## Why it matters

A library is hundreds to thousands of files. With no file named, the only
way to find the culprit is bisection. Locating one bad file among 212 took
eight compile-and-test rounds; at ~20 s per compile of a large library, the
same bisection over Buildings' 5959 files would take roughly a dozen rounds
and the better part of an hour — to recover information the compiler had
and discarded.

It also makes the failure look categorical. Every model in the library
reports the identical message, so the natural reading is "this compiler
cannot handle this library" rather than "one file has a stray byte". That
is the difference between a 0% score and a one-line fix.

## Suggested fix

Carry the failing path and the underlying parse diagnostic through the
source-root loader and report them the way a direct compile does. The
information exists at the point of failure; only the propagation is
missing. Reporting *every* file that failed, rather than the first, is
better still — a tree with ten bad files should not need ten runs.

## Reproduction

```bash
mkdir -p /tmp/lib/L && printf 'within L;\npackage L end L;\n' > /tmp/lib/L/package.mo
printf 'this is not modelica\n' > /tmp/lib/L/Broken.mo
rumoca compile /tmp/lib/L/package.mo --model L --source-root /tmp/lib
# names /tmp/lib, not /tmp/lib/L/Broken.mo
```
