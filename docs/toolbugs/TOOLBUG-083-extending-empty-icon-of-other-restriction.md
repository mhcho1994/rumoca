# TOOLBUG-083 — Extending an empty icon class of another restriction rejected (ER091)

**Status:** fixed (accepted with warning WR008).
**Severity:** low.

## What

`package TGData extends Modelica.Icons.Record;` (OpenIPSL) and
`model X extends TRANSFORM.Icons.Function;` violate the MLS §7.1.3
restriction table (package↔package, function↔function). OpenModelica accepts
them; the bases are icon classes with no elements, so inheriting them cannot
change the extending class.

Affected (cluster B): OpenIPSL CampusB (3), TRANSFORM
`Initial_FissionProducts_Test` (1) — 4 models.

## Fix

`restrictions/decl.rs::check_inheritance_compatibility`: when the incompatible
base is contentless (no components, nested classes, equations, algorithms,
external clause, and only contentless bases) the diagnostic is the warning
WR008; a base with content keeps ER091.

## Test

`overstrict_checks.rs::empty_incompatible_base_warns_non_empty_rejects`.
