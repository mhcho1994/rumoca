# TOOLBUG-035 — a directory no package references makes the whole library unusable

**Status:** open. Found compiling `Modelica_DeviceDrivers`.
**Severity:** high — fatal, whole-library, for a defect in dead weight.

## What

`PKG-006 directory '…/Communication/Packager' is missing package.mo` aborts
the compilation of *every* model in the library. The directory in question
is not part of the library:

```
$ ls Modelica_DeviceDrivers/Communication/Packager/
BitPackager.mo  MinimalSerialPackager.mo  package.order   # no package.mo

$ grep -c Packager Modelica_DeviceDrivers/Communication/package.order
0
$ grep -rn 'package Packager' Modelica_DeviceDrivers/
(nothing)
```

Its parent's `package.order` does not list it, and no `package Packager` is
declared anywhere. Nothing in the library can reach it. It is a leftover
directory in a git repository, and it costs all 165 models.

OpenModelica 1.27.1 loads the same tree and checks 137 of those 165 models
successfully. It does not look at the directory, because nothing refers to
it.

## Why the current behaviour is the wrong trade

A missing `package.mo` in a directory that *is* referenced is a real defect
and should be reported. The question is only what happens to a directory
nothing references. Three things argue for ignoring it:

- **Nothing can be affected by it.** No name resolves through it, so no
  model's meaning depends on whether it is well formed.
- **The blast radius is inverted.** The cost of the strictest possible
  reading falls entirely on models that have nothing to do with the
  offending directory.
- **The competing implementation disagrees**, and these libraries are
  published, used and simulated as they are.

At minimum this should be a warning rather than a fatal error when the
directory is unreachable from the package's own `package.order`. Refusing
*only* the unreachable subtree, and reporting it, is better still.

## Reproduction

```bash
mkdir -p /tmp/lib/L/Sub
printf 'within;\npackage L\nend L;\n' > /tmp/lib/L/package.mo
printf 'Used\n' > /tmp/lib/L/package.order
printf 'within L;\nmodel Used Real x; equation x = 1; end Used;\n' > /tmp/lib/L/Used.mo
printf 'Orphan\n' > /tmp/lib/L/Sub/package.order   # Sub/ has no package.mo
printf 'within L.Sub;\nmodel Orphan Real y; equation y = 2; end Orphan;\n' > /tmp/lib/L/Sub/Orphan.mo
rumoca compile /tmp/lib/L/package.mo --model L.Used --source-root /tmp/lib
# PKG-006 on L/Sub, although L.Used neither names nor needs it
```

## Note

Removing the directory does not make this library compile — the next
diagnostic is `ER049 connector 'PackageIn' cannot contain component 'pkg'`,
which is a per-model question and a separate matter. The point here is only
that an unreachable directory should not be able to answer for the library.
