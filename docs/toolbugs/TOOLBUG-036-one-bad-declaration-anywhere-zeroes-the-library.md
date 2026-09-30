# TOOLBUG-036 — one bad declaration anywhere in a source root zeroes every model

**Status:** fixed for all three triggers (2026-09-30). BOM: TOOLBUG-033. Unreferenced directory: TOOLBUG-035. `outer` with a modification: no longer a parse error -- WR007 reports it and resolution drops it (MLS §5.4), as OpenModelica does. Unparseable files elsewhere in a root are skipped with a warning.
**Severity:** high — turns a local defect into a total one.

## What

`--source-root` parses and validates the whole tree before compiling
anything, and any failure anywhere aborts every model. A model that neither
names nor needs the offending code fails identically to one that does.

Measured on the evaluation dataset, three unrelated triggers each produced
the same total outcome:

| Trigger | Where | Libraries | Models at 0% |
|---|---|---:|---:|
| UTF-8 BOM ([TOOLBUG-033](TOOLBUG-033-a-utf8-bom-makes-a-file-unparseable.md)) | 50 files | 4 | — (normalised before the run) |
| Unreferenced dir with no `package.mo` ([TOOLBUG-035](TOOLBUG-035-an-unreferenced-directory-is-fatal-to-the-library.md)) | 1 directory each | 4 | 2783 |
| `outer` component with a modification | 1 declaration | 1 | 2062 |

**4845 of 13028 targets — 37% of the dataset — scored zero because of one
file, one directory, or one declaration.** OpenModelica checks 87% of the
same models.

## The third trigger

```
IDEAS/BoundaryConditions/Occupants/Extern/StROBe.mo:8
  outer StrobeInfoManager strobe(final StROBe_P=true, StROBe = true)

EP001 parse error: Outer component 'strobe' shall not have modifications
```

The rule is real — MLS §5.4 forbids a modification on an `outer` element, so
IDEAS is non-conforming here and rumoca is right to object. Two things are
still wrong with the outcome:

- It is enforced **in the parser**, as `EP001`, so it cannot be a warning,
  cannot be scoped to the class that contains it, and aborts the tree.
- The cost lands on 2061 models that have nothing to do with `StROBe.mo`.

## Why this is one defect and not three

The triggers are unrelated and each deserves its own fix. The *amplification*
is a single architectural property: there is no fault isolation at library
scope. Fixing the three triggers leaves the next bad file to do the same
thing — and real libraries always have one.

## Suggested fix

Isolate failures to the compilation unit that contains them. A file that
fails to parse, or a directory that fails structural validation, should
poison the classes it defines and nothing else; a model that resolves
without touching them should compile and say so. Report every such failure
at the end, so one run enumerates the tree's problems instead of surfacing
them one bisection at a time
([TOOLBUG-034](TOOLBUG-034-source-root-parse-errors-do-not-name-the-file.md)).

This is what makes a compiler usable against third-party libraries, which
are never clean.
