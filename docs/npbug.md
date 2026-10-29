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
- #2935 (NPExt changelog item 9): the AI super-weapon rally pick
  (`AI_FindBestRallyTarget50CBF0`, choice `0x0050CF38..0x0050D16B`) skips a
  fully cloaked entry instead of letting its `RandomRanged(0, best + 10)`
  draw beat every real candidate.
- #2936 (NPExt changelog item 11): insufficient house power refuses the
  pre-selected ChronoWarp click (`ClickFire4FB1E0` admission, the PostClick=
  exemption at `0x006CB933..0x006CB95F`) before any launch or paired Sphere
  write, so the warp neither fires nor restarts the countdown while held.
- #2938 (NPExt changelog item 80): the Psychic Dominator strike's area
  damage books its kills to the Dominator's owner. Native passes the
  lightning-storm house slot (`0x00A9FACC`) — empty without an active
  storm — so building kills reached no score and no destroyed-buildings
  count.

### Combat target, order, fire and damage eligibility

- #2937 (NPExt changelog item 41): the passive acquisition scan
  (`TechnoClass::Greatest_Threat @ 0x006F8DF0`'s candidate gates) refuses an
  invulnerable candidate. Native gates G1..G13 have no invulnerability arm —
  the Iron Curtain is enforced receiver-side only — so auto-acquisition
  dumped fire into curtained units.
- #2942 (NPExt changelog item 7): a temporal erase pays one alliance-gated
  veterancy award. Native pays the Update's `VeterancyStruct::Add`
  (`0x0071A917..0x0071A978`, no alliance test) and Record_The_Kill's award
  (`0x00702D40`, zeroed for allies) together — friendly victims paid and
  enemy victims paid twice.
- #2945 (NPExt changelog item 54): a NULL `Assign_Target` (`0x006FCF5B`)
  mid-burst keeps the burst slot instead of resetting it, so the
  interrupted burst resumes and the full ROF still separates bursts — the
  cancellation no longer skips the reload.
- #2955 (NPExt changelog item 10): the guard hotkey's miner fallback
  (`Guard730D60`'s Unit slave-manager/Harvester arm) demands the issuing
  house's ownership. Native pairs that ownership-free arm with the MegaMission
  executor's owner-free retask (Queue `0x004C73B9`), so a foreign harvester
  caught in the selection could be stalled on area guard.
- #2961 (NPExt changelog item 72): the fire-error cell arm
  (`0x006FC7EB..0x006FC868`) consults the naval selector — a
  `NavalTargeting=6` firer refuses the water surface itself, closing the
  open-water force-attack the Techno-only T40 gate missed.
- #2963 (NPExt changelog item 96): the crash latch joins the fall refusal
  (fire-error T19/T35) — a crashing Rocketeer can neither fire nor be
  attacked. Native's fall state covered only the parachute/bomb/flaming
  fall, so crash-latched aircraft absorbed fire and hit the ground
  shooting.
- #2968 (NPExt changelog item 65): damage landing exactly on the yellow
  threshold classifies as the `DamageState::Yellow` crossing, so the struck
  building plays its damaged-building cue. Native `Object5F5390` tests the
  post-damage HP strictly (`post < strength >> 1`), so the exact boundary
  stayed Damaged and the `BuildingClass::ReceiveDamage @ 0x00442230` result
  table (`0x00442476`) never reached the global `BuildingDamageSound` request
  (`0x00442706`).
- #2981 (NPExt changelog item 55): a kill's veterancy award divides by the
  recipient's OWN house cost factors. Native `Record_The_Kill`
  (`0x00702ED1..0x00702FD4`) prices every recipient arm through the victim's
  house, so industrial plants or country discounts on the victim's house
  skewed the killer's promotion speed.

### Weapon, wave and projectile source, path and range

- #2944 (NPExt changelog item 14): a wave remembers the slot whose FireAt
  emitted it. Native re-reads GetFLH(0) (`0x00762AF0`) for the geometry and
  GetWeapon(0) for the sonic cell damage, so a non-primary emitter anchored
  its visuals and paid the primary's AmbientDamage/warhead. Under npbug both
  follow the emitting weapon.
- #2946 (NPExt changelog item 59): the GetROF early return for sonic
  weapons and live spark/fire/railgun systems (`0x006FCFE0..0x006FD036`)
  also takes the ROF-ability reduction (`0x006FD0E2..0x006FD14C`), so
  veteran and elite dolphins, squids and effect weapons keep their reload
  bonus.
- #2947 (NPExt changelog item 44): a `Suicide=` weapon works from any slot
  — the shot launches first and the self Strength packet lands after it
  (`Techno6FDDC0..6FDE0E`'s branch no longer returns before the launch), so
  the weapon deals its own damage and the death still completes the nested
  DeathWeapon. NPExt's exact slot gating was not disassembled; every slot
  takes the corrected flow as a recorded residual.
- #2948 (NPExt changelog item 84): the InRange arming ceiling over
  structural bridges (`0x006F74D7..0x006F7504`) consults the projectile's
  `SubjectToCliffs=` — a projectile that ignores cliffs may shell a deck
  three or more levels up; cliff-respecting arcing shots stay refused.
- #2949 (NPExt changelog item 43): a Burst=1 direct shot advances the burst
  index, so the FLH parity alternates launch sides. Native's remainder
  (fire_coord parity read; Aegis `Burst=1` Medusa) never advanced, so every
  missile left from the same side.
- #2951 (NPExt changelog item 99): a missile whose target dies while
  landing or docking loses the target instead of taking the pad's ground
  cell. Native's PointerExpired repair (`LoseTarget468430` /
  `PointerExpired468517` cell substitution) treats the low aircraft as
  ordinary, so anti-air fire that downs a landing aircraft flew into the
  airbase.
- #2986 (NPExt changelog item 85): the magnetic wave takes the
  tracking-distance decay (`MAX_TRACKING_DISTANCE_LEPTONS`). Native skipped
  type 3, so a magnetron beam followed its held target at any distance; the
  decay ends the beam like every other wave type.

### Building placement, construction, economy and sensors

- #2930 (NPExt changelog item 53): the selected-building ring radius
  (`GetSensorRange @ 0x004566B0`) no longer consults `SensorArray=` — a
  `SensorArray=yes` building falls through to its normal weapon-range radius
  instead of drawing `CloakRadiusInCells=`. The cloak-generator arm keeps
  native behavior.
- #2953 (NPExt changelog item 17): a naval factory's rally click resolves
  only against water. Native resolved the player rally through the
  amphibious movement zone, so land tiles in that zone were accepted and
  beached the ships.
- #2954 (NPExt changelog item 49): the shared exit owner
  (`exit_produced_object`) refuses a producer that is still constructing
  (Building+534 == 0) or selling, returning to the placement wait. Native
  ran the finished-object exit before the constructing gate — which only
  guarded starting production — and the Strip PLACE tail had no
  construction admission either, so vehicles left unfinished factories
  and stuck inside them.
- #2965 (NPExt changelog item 6): a refused `UndeploysInto=` conversion
  (`BuildingClass::Mission_Selling @ 0x00449C30`, the Unlimbo refund tail
  `0x0044A16B..0x0044A176`) deletes the never-placed limbo Unit with the
  building. Native retains it, so its `Add_Tracking @ 0x004FF700` count kept
  feeding ShortGame's BaseUnit sum and the defeat gate
  (`HouseClass::Update` `0x004F8E92..0x004F8F77`) never declared the emptied
  house defeated.
- #2966 (NPExt changelog item 34): a `CanBeOccupied=` building refuses every
  entrant at or below red health, owner included
  (`BuildingClass::CanBeOccupiedBy @ 0x00457CE0`, `CanDock @ 0x00457D3E`), so
  infantry released from a battle bunker their own house let drop to red could
  never re-garrison it. Under npbug the building's own house is exempt;
  other houses keep the refusal.
- #2971 (NPExt changelog item 95): a tech building (`NeedsEngineer=`) starts
  its active animations only when a real house owns it. Native fills
  Idle18 + Active3..6 at first opening (`0x0044632E..0x00446374`) with no
  ownership arm, so an un-owned oil derrick pumps from scenario placement and
  keeps pumping after the damaged-art swap; the capture's `Grand_Opening`
  re-run (Building `0x00448CEF`) starts it. A building transferred back to a
  passive house keeps the slots it already has.
- #2978 (NPExt changelog item 18): the prism supporter walk refuses a tower
  still under construction (Building+534 == 0). Native asked
  alive/same-type/rearm/idle-ish but never the build state, so unfinished
  towers contributed beams during base build-up.

### Movement, crash, ejection, garrison and lifecycle

- #2934 (NPExt changelog item 4): the `MakeInfantry=` anim end
  (`AnimClass::AI @ 0x00423AC0`, refused-Unlimbo branch `0x00424B23`) frees
  the failed creation instead of leaving it in limbo. Natively a Genetic
  Mutator over an occupied cell allocated a fresh brute on every completed
  frame and never destroyed the refused one — unbounded object growth while
  the cell stayed blocked. The anim still steps its stage back and retries.
- #2956 (NPExt changelog item 19, part 1 of 4): a unit the Magnetron lifts
  is not self-propelled — held locomotion plays no move sounds. Native ran
  the victim's own foot move-sound tail over the held jumpjet route. (The
  lift speed, drop crash sounds and held-aircraft death parts of the chain
  remain open.)

### Capture, mind control, lift and retaliation

### UI, presentation, audio and scenario lifetime

- #2931 (NPExt changelog item 79): scenario transitions clear the screen
  shake. Native's offset lives in a process-global that only decays while
  battle input runs, so quitting mid-shake started the next game with
  leftover camera movement.
- #2952 (NPExt changelog item 92): the attack rung refuses an unforced
  attack on a target the selected warhead cannot affect through vanilla
  receiver-side immunity — `Radiation=yes` against `ImmuneToRadiation=`,
  `Psychedelic=yes` against `ImmuneToPsionics=` / `ImmuneToPsionicWeapons=`
  (native `0x00700255..0x0070043E` tests only the type's `Immune=` flag).
  A forced order stays the override; NP-specific immunity flags and a
  vanilla virus pair are out of scope.
- #2979 (NPExt changelog item 28): the action determination's warp rung also
  catches a still-warping-in actor over an entity target. Native limits the
  warp-in arm to Cell targets, so a pre-emergence chrono legionnaire fell
  through to the attack rung's pure weapon-range alternative and showed a
  force-attack cursor over friendlies.
- #2980 (NPExt changelog item 45): the T type-select seed set no longer grows
  the `DeploysInto`/`UndeploysInto` counterpart type (the 0x00732C30 ID match
  reads it at 0x00732C61..0x00732C83), so T with an MCV selected gathers MCVs
  only instead of also taking the Construction Yard, and vice versa.
- #2984 (NPExt changelog item 70): the mind-control removal sound plays
  only for the two involved parties — the (former) controller and the
  owner the unit returns to. Native emitted a plain positional VocAt with
  no house filter, so every player heard it.
