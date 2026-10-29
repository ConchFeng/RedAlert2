# Optional NPBUG bug-fix extension

NPBUG is VERA20K's own opt-in compatibility extension. It collects gameplay
corrections adopted from the NPExt (NPatch Extended) bug-fix changelog: each
chain fixes a defect the original game carries, so enabling NPBUG is a
deliberate, documented deviation from native behavior, not a parity fix.

The extension exists because the reports are compatibility candidates, not
VERA-versus-retail defects — the inventory and its policy live in
[#3024](https://github.com/YuriPlanet/vera20k/issues/3024). Builds without the
feature keep every native path and its pinned goldens.

Enable it explicitly:

```sh
cargo run --release --bin vera20k --features npbug
```

The feature compiles to `rules::NPBUG_ENABLED`. A save or network session
recorded with NPBUG cannot load or join in a build without it, matching the
other capabilities (`RulesCompatibility`).

## Chains

Each implemented fix records its issue, the NPExt changelog item it adopts,
and the native site the defect lives at, next to the Rust that changes it.
The sections follow the behavior chains of #3024.

### Rules and repeated INI reads

- #2928 (NPExt changelog item 5): a `[General]` pass that omits
  `PrismSupportModifier=` keeps the retained percentage instead of
  multiplying it by another 100 (`ReadGeneral67114F`, scale at
  `0x0067116E`). NPBUG compiles the same default rescale the Ares hook
  `RulesClass_Addition_General_PrismSupportModifier` (`0x1005D040`) uses.
- #2929 (NPExt changelog item 88): re-defining a type without `Pip=` /
  `OccupyPip=` keeps the retained pip frames; native `ReadPip4748A0`
  re-derives the default name from the retained frame's slot in the
  adjacent name array (`0x0081B9B0`), drifting every keyless pass.

### Superweapon and AI strategy

- #2933 (NPExt changelog item 86): the secret-lab startup assignment draws
  uniformly over the choices that remain, removes the granted one, and
  refills the whole list for further labs. Native
  `AssignSecretLabProduction68C050` (`68C112..68C1B5`) uses the drawn
  position against the original arrays — shrinking range, duplicate grants —
  and returns early when labs outnumber types, leaving every lab inert.
  NPExt's exact rewrite was not disassembled; this implements the corrected
  contract the report states, with a changed RNG draw pattern as a recorded
  residual.

### Combat target, order, fire and damage eligibility

### Weapon, wave and projectile source, path and range

### Building placement, construction, economy and sensors

- #2930 (NPExt changelog item 53): the selected-building ring radius
  (`GetSensorRange @ 0x004566B0`) no longer consults `SensorArray=` — a
  `SensorArray=yes` building falls through to its normal weapon-range radius
  instead of drawing `CloakRadiusInCells=`. The cloak-generator arm keeps
  native behavior.

### Movement, crash, ejection, garrison and lifecycle

- #2934 (NPExt changelog item 4): the `MakeInfantry=` anim end
  (`AnimClass::AI @ 0x00423AC0`, refused-Unlimbo branch `0x00424B23`) frees
  the failed creation instead of leaving it in limbo. Natively a Genetic
  Mutator over an occupied cell allocated a fresh brute on every completed
  frame and never destroyed the refused one — unbounded object growth while
  the cell stayed blocked. The anim still steps its stage back and retries.

### Capture, mind control, lift and retaliation

### UI, presentation, audio and scenario lifetime

- #2931 (NPExt changelog item 79): scenario transitions clear the screen
  shake. Native's offset lives in a process-global that only decays while
  battle input runs, so quitting mid-shake started the next game with
  leftover camera movement.
