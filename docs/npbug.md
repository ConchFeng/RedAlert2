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
  `PrismSupportModifier=` keeps the retained percentage exactly instead of
  multiplying it by another 100 (`ReadGeneral67114F`, scale at
  `0x0067116E`). An Ares build keeps Ares's own rescale
  (`RulesClass_Addition_General_PrismSupportModifier`, `0x1005D040`), which
  chops most values by one per keyless pass.

### Superweapon and AI strategy

- #2933 (NPExt changelog item 86): the secret-lab startup assignment draws
  uniformly over the choices that remain, removes the granted one, and
  refills the whole list for further labs. Native
  `AssignSecretLabProduction68C050` (`68C112..68C1B5`) uses the drawn
  position against the original arrays — shrinking range, duplicate grants —
  and returns early when labs outnumber types, leaving every lab inert.
  NPExt's exact rewrite was not disassembled; this implements the corrected
  contract the report states. The draw ranges match native while labs do not
  outnumber types; each refilled lab adds one draw.
- #2935 (NPExt changelog item 9): the AI super-weapon rally pick
  (`AI_FindBestRallyTarget50CBF0`, choice `0x0050CF38..0x0050D16B`) skips a
  fully cloaked entry instead of letting its `RandomRanged(0, best + 10)`
  draw beat every real candidate.
- #2936 (NPExt changelog item 11): a pre-selected ChronoWarp click is
  refused, with no write, while its paired Sphere is held (IsSuspended, which
  `Update_Owned_Supers @ 0x0050AF10` sets on the IsPowered Sphere for low
  power or no online provider). Native `Fire_SW @ 0x004FAE50` launched the
  warp through ClickFire's PostClick exemption (`0x006CB933..0x006CB95F`),
  then cleared the Sphere's readiness and restarted its countdown while held
  (`0x004FAEA6..0x004FAEC1`).
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
  dumped fire into curtained units. The capture and occupy scans (masks
  `0x200`, `0x8000`, `0x10000`) keep native.
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
  (`0x006FC7EB..0x006FC868`) reads only LandTargeting, so a `NavalTargeting=6`
  firer (dogs, Terror and Chaos Drones) could force-attack a water or beach
  cell. Under npbug that firer type (`+0x600`) refuses the cell; every other
  firer is unchanged.
- #2968 (NPExt changelog item 65): HP exactly on `strength >> 1` counts as
  the yellow side, so the hit landing on it is the `DamageState::Yellow`
  crossing (yellow Tags 39/42, the damaged-building cue) and the next hit
  from it is no second crossing. Native `Object5F5390` answers Yellow only
  for `half <= previous && post < half`, so the exact boundary stayed
  Damaged and the `BuildingClass::ReceiveDamage @ 0x00442230` result table
  (`0x00442476`) never reached the global `BuildingDamageSound` request
  (`0x00442706`).
- #2981 (NPExt changelog item 55): a kill's veterancy award divides by the
  recipient's OWN house cost factors. Native `Record_The_Kill`
  (`0x00702ED1..0x00702FD4`) prices every recipient arm through the victim's
  house, so industrial plants or country discounts on the victim's house
  skewed the killer's promotion speed.

### Weapon, wave and projectile source, path and range

- #2946 (NPExt changelog item 59): the GetROF early return for sonic
  weapons and live spark/fire/railgun systems (`0x006FCFE0..0x006FD036`)
  also takes the ROF-ability reduction (`0x006FD0E2..0x006FD14C`), so
  veteran and elite dolphins, squids and effect weapons keep their reload
  bonus.
- #2948 (NPExt changelog item 84): the InRange arming ceiling over
  structural bridges (`0x006F74D7..0x006F7504`) consults the projectile's
  `SubjectToCliffs=` — a projectile that ignores cliffs may shell a deck
  three or more levels up; cliff-respecting arcing shots stay refused.
- #2951 (NPExt changelog item 99): a missile whose aircraft target dies, or
  expires while it has a home pad and is not in the air, loses the target
  instead of taking the ground cell. Native's PointerExpired repair
  (`LoseTarget468430` / `PointerExpired468517` cell substitution) treats the
  low aircraft as ordinary, so anti-air fire that downs a landing aircraft
  flew into the airbase. The pad (Aircraft+0x6CC) stays set for the
  aircraft's life, so parked and departing aircraft take the arm too.
- #2986 (NPExt changelog item 85): the magnetic wave decays once its held
  target leaves its owner's weapon 0 `Range=`. Native's tracking decay
  (`WaveClass::Update_Wave @ 0x00762AF0`) skips type 3, so a magnetron beam
  followed its target at any distance. The other types' 2172-lepton bound
  would end beams inside the retail MagneticBeam's 12-cell range.

### Building placement, construction, economy and sensors

- #2930 (NPExt changelog item 53): the selected-building ring radius
  (`GetSensorRange @ 0x004566B0`) no longer consults `SensorArray=` — a
  `SensorArray=yes` building falls through to its normal weapon-range radius
  instead of drawing `CloakRadiusInCells=`. The cloak-generator arm keeps
  native behavior.
- #2953 (NPExt changelog item 17): a naval factory's rally accepts only a
  resolved water cell. Native `SetRallyPoint` (`0x00443860`) resolves the
  click in the Amphibious zone, so land tiles in that zone were accepted and
  beached the ships. A refused click still plays the rally announcement, and
  an opening shipyard whose offset cell is land gets no rally.
- #2965 (NPExt changelog item 6): a refused `UndeploysInto=` conversion
  (`BuildingClass::Mission_Selling @ 0x00449C30`, the Unlimbo refund tail
  `0x0044A16B..0x0044A176`) deletes the never-placed limbo Unit with the
  building. Native retains it, so its `Add_Tracking @ 0x004FF700` count kept
  feeding ShortGame's BaseUnit sum and the defeat gate
  (`HouseClass::Update` `0x004F8E92..0x004F8F77`) never declared the emptied
  house defeated. This is part of #2965: undeploying a yard already being
  sold is a separate VERA-versus-native gap.
- #2966 (NPExt changelog item 34): a `CanBeOccupied=` building refuses every
  entrant at or below red health, owner included
  (`BuildingClass::CanBeOccupiedBy @ 0x00457CE0`, `IsRedHP` at `0x00457D8F`), so
  infantry released from a battle bunker their own house let drop to red could
  never re-garrison it. Under npbug the building's own house is exempt;
  other houses keep the refusal.
- #2971 (NPExt changelog item 95): a tech building (`NeedsEngineer=`) owned
  by a MultiplayPassive house skips Grand_Opening's Idle18 + Active3..6 fill,
  and the capture's `Grand_Opening` re-run (`BuildingClass::ChangeOwner`,
  `0x00448CEF`) fills those empty slots once a non-passive house owns it.
  Native fills them at first opening (`0x0044632E..0x00446374`) with no
  ownership arm, so an un-owned oil derrick pumped from scenario placement.
  Placement for a real house keeps the native fill; a building handed back
  to a passive house keeps the slots it has.
- #2978 (NPExt changelog item 18): the prism supporter walk
  (`0x0044B357..0x0044B4BD`) refuses a tower in body state 0
  (Building+534), so unfinished towers no longer contribute beams during
  base build-up. A tower being sold is in the same state and is refused too.

### Movement, crash, ejection, garrison and lifecycle

- #2934 (NPExt changelog item 4): the `MakeInfantry=` anim end
  (`AnimClass::AI @ 0x00423AC0`, refused-Unlimbo branch `0x00424B23`) frees
  the failed creation instead of leaving it in limbo. Natively a Genetic
  Mutator over an occupied cell allocated a fresh brute on every completed
  frame and never destroyed the refused one — unbounded object growth while
  the cell stayed blocked. The anim still steps its stage back and retries.
- #2956 (NPExt changelog item 19, part 1 of 4): a unit the Magnetron lifts
  is not self-propelled — held locomotion plays no move sounds. Native ran
  the victim's own foot move-sound tail over the held jumpjet route
  (`FootClass::AI @ 0x004DA530`); the skip also skips that tail's Main RNG
  draw (`0x004DAACB`). The lift speed, drop crash sounds and held-aircraft
  death parts of the chain remain open.

### Capture, mind control, lift and retaliation

### UI, presentation, audio and scenario lifetime

- #2931 (NPExt changelog item 79): scenario transitions clear the screen
  shake. Native's offset is the global Map's `+4`/`+8` (`0x0087F7E8`), which
  only decays while battle input runs (`0x004F4BB0`), so quitting mid-shake
  started the next game with leftover camera movement.
- #2979 (NPExt changelog item 28): the action determination's warp rung also
  catches a still-warping-in actor over an entity target. Native limits the
  warp-in arm to Cell targets, so a pre-emergence chrono legionnaire fell
  through to the attack rung's pure weapon-range alternative and showed a
  force-attack cursor over friendlies. The rung answers every object target
  while warping in, enemies included.
- #2980 (NPExt changelog item 45): the T type-select seed set no longer grows
  the `DeploysInto`/`UndeploysInto` counterpart type
  (`TypeSelect__BuildSeedTypeIDs @ 0x00732D10`, Type+0x404/+0x408), so T with
  an MCV selected gathers MCVs only instead of also taking the Construction
  Yard, and vice versa. An Ares build follows `TypeSelectUseDeploy=`.
- #2984 (NPExt changelog item 70): the mind-control removal sound plays
  only for the two involved parties — the (former) controller and the
  owner the unit returns to. Native emitted a plain positional VocAt with
  no house filter, so every player heard it.
