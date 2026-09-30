# TOOLBUG-085 — External-object members of records/connectors rejected (ER023/ER049)

**Status:** fixed at resolve (accepted with warning WR011).
**Severity:** low.

## What

Modelica_DeviceDrivers passes its `SerialPackager` handle (a class extending
`ExternalObject`) through `connector PackageOut ... output SerialPackager pkg;`
and keeps a `Comedi` handle in a configuration record. MLS §4.7/§9.1 list only
record/type (and connector) component types there; OpenModelica accepts
external objects.

Affected (cluster B): `Blocks.Communication.SocketCAN.ReadMessage`,
`Blocks.Examples.TestHardwareIOComedi`, `ClockedBlocks.Examples.TestHardwareIOComedi`,
`Blocks.Examples.TestSerialPackager_ExternalTrigger` — 4 models.

## Fix

`semantic_checks/mod.rs`: a component whose type is an external object class
(a `class` directly extending `ExternalObject`) in a record or connector is
the warning WR011 instead of ER023/ER049.

## Test

`overstrict_checks.rs::external_object_record_and_connector_members_warn`.
