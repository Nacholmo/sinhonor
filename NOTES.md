# sinhonor — research notes

**Scope:** a **portable Dishonored motion kit**, in the spirit of how
[2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) drops Skate 3 skating into IW4L.
It covers player motion (walk, sprint, crouch, jump, fall, slide, mantle, lean, swim, ladder climb, and the camera feel)
and the **motion powers**: Blink, plus Agility's power jump and fall-damage changes. All of it is packaged to be modded
into *other* games. This is *not* a full rewrite of Dishonored. AI, general combat, non-motion powers, UI, saves, audio and the
campaign are out of scope. They appear below only where motion or Blink touches them (for example, blink knockdown damage types).
Some combat is in scope because it pairs with motion and Blink: the **drop assassination** (§5b), the basic **sword
attack** (§5c) and the **ground assassination** (§5d).

Intended shape (mirrors the mashup's `skate/` subsystem): an engine-agnostic motion and Blink crate that asks the host
for collision through a trait (sweeps, traces, ledge probes); a data crate that reads tuning, tweaks and animation from
the user's own Dishonored install; and a small reference host plus adapters for host games.

Prior-art survey for a Rust reimplementation of Dishonored (2012) player motion and Blink. The game data
is read at runtime from the user's own install. Nothing from the game
(assets, configs, decompiled script or native code) is copied into this repo. These notes record
only *where* things live and *how* the formats work, along with the source of each finding.

Survey date: 2026-10-06.

## 1. What the install looks like

- Engine: Unreal Engine 3, retail build. Package header: tag `0x9E2A83C1`, file version **801**, licensee **30**,
  engine version **9411**, cooker **133**. Every `.upk` checked uses compression flag `2` (LZO chunks), so packages must be
  decompressed chunk by chunk before parsing.
- `DishonoredGame/CookedPCConsole/`: about 470 `.upk` (UE3 packages), about 311 `.pck`, one `.bnk` (Wwise audio), and 3 `.tfc`
  (texture file caches).
  - Script/class packages: `Core.upk`, `Engine.upk`, `GameFramework.upk`, **`DishonoredGame.upk`** (game classes,
    including `DishonoredActivePowerComponent_Blink`).
  - Levels are split per map: `L_<map>_P`, `_Script`, `_Nav`, `_FX`, `_Audio`, and so on.
- `DishonoredGame/Config/Default*.ini` are **plain-text INI** files and hold tuning values for the `config` properties of the
  game classes. The relevant ones:
  - `DefaultPower.ini` → `[DishonoredGame.DishonoredActivePowerComponent_Blink]`: separate max horizontal and vertical
    distance (the "squashed sphere" reach), step distance and time between steps (the destination is found by
    **iterative stepping**, not a single trace), big and small step length and max angle, a ledge check distance scale,
    warmup and cooldown, bend-time down and up times, and targeting indicator sizes. Also BendTime, WindBlast, and soul pickup settings.
  - `DefaultPlayer.ini` → `DishonoredPlayerPawn`, and `DishonoredPowersComponent` per-level power modifiers. Agility
    ("Celerity") is expressed as named attribute modifiers such as `Attribute_JumpZ_PowerJump` and `Attribute_GroundSpeedSprint`.
  - `DefaultPlayerState.ini` → the player movement state machine: `StatePlayerMaster{Walk,Jump,Falling,Climb,Mantle,
    Leaning,Swim,...}`.
  - `DefaultCamera.ini` → camera modifiers (lean, dodge, bump smoother, mantle offsets, ...).
  - `DefaultInput.ini` lists dev-console commands (`BlinkShowRange`, `ShowDebug Blink`, `AddPower Blink <level>`).
- `Binaries/Win32/Dishonored.exe`: 32-bit and about 18 MB. Middleware: PhysX 2.8.x, APEX, Wwise, Bink, Scaleform.

## 2. Existing tools and documentation

| Project | What it gives us | License |
|---|---|---|
| [EliotVU/Unreal-Library (UELib)](https://github.com/EliotVU/Unreal-Library) | UE1–3 package reader and **UnrealScript decompiler**. Has an explicit `Dishonored` build (801/30) with Dishonored-specific `UClass` serialization (an extra name ref and a `NativeClassName` string). | MIT |
| [UE-Explorer](https://github.com/UE-Explorer/UE-Explorer) | GUI on top of UELib. Lists Dishonored as supported. | GPL-3.0 |
| [gildor2/UEViewer (umodel)](https://github.com/gildor2/UEViewer) | Mesh and texture viewer/exporter. Dishonored supported since 2012 **except animations**. Documents the Dishonored-specific `USkeletalMesh` fields (`m_UserBounds`, `EdgeSkeleton` at ver ≥ 775, old kDOP layout). | MIT |
| [deadYokai/ue3-tools](https://gitlab.com/deadYokai/ue3-tools) | **Rust** UE3 package reader and packer built against Dishonored. Has docs: `package-format.md`, `property-decoding.md`, `asset-payloads.md`, and an LZO decompressor. Successor to [dishonored-toolkit](https://github.com/deadYokai/dishonored-toolkit) (Python, archived). | AGPL-3.0 (do not vendor; read docs only) |
| [ectrc/dishonoredrecompiled](https://github.com/ectrc/dishonoredrecompiled) | Functional recompilation of the native exe (boots into the first mission). Its `resources/docs/` are the best format write-ups that exist, notably **`edgeanim.md`** (see §3). | AGPL-3.0, **see provenance caveat** |
| [CodeRedModding/CodeRed-Generator](https://github.com/CodeRedModding/CodeRed-Generator) | UE3 SDK generator with a `Engine/Dishonored` profile. It dumps runtime class layouts from the running game. | MIT |
| [ectrc/dismod](https://github.com/ectrc/dismod) | Hooks the running retail game. Useful for checking behaviour against the real thing. | none stated |
| Cheat/trainer repos ([Crayfry](https://github.com/Crayfry/Dishonored_Cheat_menu), [gh1593](https://github.com/gh1593/DishonoredTrainer) GPL-3.0, [ectrc/dishonored](https://github.com/ectrc/dishonored)) | Memory pokes for blink distance, height, cooldown, and blink marker. They show which runtime fields matter but contain no logic. | mixed |

There is no public, clean-room spec of Dishonored's gameplay logic, and most of it is not in the game's UnrealScript
(see §3).

**Provenance caveat:** `dishonoredrecompiled` is built on `CodeRedModding/UnrealEngine3` (described as full UE3 2013
source, which is Epic's proprietary code) and on IDA decompiles of a symbolized 2012 QA build. Treat it as a pointer to *facts*
(field names, layouts, which functions are native) and never copy code from it. Its AGPL license would also
apply to anything copied.

## 3. Where each thing lives

- **Blink.** `DishonoredGame.DishonoredActivePowerComponent_Blink` (subclass of `DishonoredActivePowerComponent`,
  `UActorComponent`, config file `Power`). Its reflected state includes a blink mode, original location, rotation,
  velocity and physics (blink is restored or aborted against these), the step count and last step direction, touched pawns,
  "distance above ground", targeting ground and goal points and normals, and ledge, impulse and step-angle tuning.
  **Verified 2026-10-06: the logic is native C++.** The class is declared `native(Power) config(Power)` and has
  **zero script functions** in `DishonoredGame.upk`: only properties, an `EBlinkMode` enum and default objects.
  (An earlier guess in this file, that the logic was UnrealScript bytecode, was wrong.)
- **Movement.** Also **native**. `StatePlayerMaster{Walk,Jump,Falling,Mantle,Climb,Base,...}`,
  `DishonoredNativeStateMachine`, `DishonoredActivePowerComponent` and `DishonoredPowersComponent` have no script
  functions. On top of UE3's native `PHYS_Walking/Falling` there is a custom "LocoNew" walking path
  (`execLocoNewPhysWalking` in the cheat manager). About 1,100 functions exist in the whole game script package, out of
  30k exports. Script is mostly data and glue; the gameplay rules are in `Dishonored.exe`.
- **Animation.** Cooked `UAnimSequence`s store a **Sony Edge Animation** blob (PS3 SPU format, tag `"50AE"`,
  compression format id 7), and every `USkeletalMesh` carries an Edge skeleton blob (tag `"30SE"`). Joint *i* of the Edge
  skeleton equals UE bone *i*. Key data is big-endian and bit-packed. This is why umodel can't play Dishonored animations.
  The layout is described in prose in `dishonoredrecompiled/resources/docs/edgeanim.md`. `crates/edge_anim` is a clean
  implementation from that description (§6d).

## 4. Findings from the local install (2026-10-06)

The method and tools live in `context/` (gitignored). Only the facts are recorded here, not values or code.

**Package decompression.** A fully compressed UE3 package stores its summary raw, followed by
`FCompressedChunk {uncompOffset, uncompSize, compOffset, compSize}` entries. Each chunk at `compOffset` is
`{tag 0x9E2A83C1, blockSize, compSize, uncompSize, {compSize, uncompSize}[n], LZO1X block data...}`. Flattening a
package means rewriting the summary with `CompressionFlags = 0` and no chunks, clearing `PKG_StoreCompressed`
(`0x02000000`), and writing each chunk's output at its `uncompOffset`. Absolute offsets survive this; the shrunken summary
is zero-padded. UELib then reads the result directly (it does not do LZO itself). Rust crate used locally: `lzokay-native` (MIT).

**Where blink data lives (read at runtime, never copied):**
- The `Default__DishonoredActivePowerComponent_Blink` CDO and `Default__DishonoredPlayerPawn.PowerBlink` live in `DishonoredGame.upk`,
  and the `config(Power)` values are overridden by `Config/DefaultPower.ini`.
- **Per-tier gameplay tuning** is in a `DisTweaks_Blink` object, `Twk_Powers.Blink.Twk_Blink`, cooked into `DishonoredGame.upk`
  and referenced by the component's `m_pPowerTweaks`. `m_Levels[]` is an array of `PowerAttributes_Blink`
  {`m_Distance`, `m_HorizDistance`, `m_VertDistance`, `m_fDefaultBlinkStepDistance`, `m_fDefaultTimeBetweenBlinkSteps`,
  warmup, cooldown and bend-time timings, warmup wobble, distortion and blur strengths, `m_fMoveReachMaxAtPercentage`, sound events}.
  Top-level fields: `m_TargetTestExtent` (the collision box used for target tests), `m_fCloseCollisionDistance` and
  `m_fCloseCollisionOffsetStep`, `m_fFallThreshold`, `m_fGroundMeshHeight`, `m_bLimitVerticalDistanceFromGround`,
  `m_bTargetThroughAwarePawns`, `m_bKnockdownAwarePawns`, `m_fKnockdownMinDistance`, mana and rune costs, and
  marker meshes and particle systems (stand, ground, mantle; low and high fall).
- The tier I and tier II horizontal reaches in `Twk_Blink` **differ from** the INI's `m_fDefaultMaxHorizDistance`.
  **Resolved in §5: the tweak object wins.** The INI `m_fDefault*` values are overwritten every time a cast starts.
- The Blink post-process parameters are named `BlinkCooldownTime`, `BlinkDistancePercentage`,
  `BlinkDistanceTravelled`, `BlinkLensIntensity`, `BlinkLocalDirection`, `BlinkStepProgress`, `BlinkStepsTaken`,
  `BlinkTargeting`, `BlinkTimeElapsed`, `BlinkWarmupTime`, `Blink_opacity`. Damage types: `DisDamageType_BlinkPush`
  and `DisDamageType_BlinkKnockdown`.

**Animation.** `Startup.upk` holds the first-person player rig: `Ply_Player.Skm_Player`, the AnimSets
`Ply_Empty_Locomotion_as`, `Ply_Powers_as`, `Ply_Generic_as`, `Ply_Sword_*` and others, plus the AnimTrees
`Ply_Player_at`, `Ply_Player_Nav_{Default,Crouched,Sprinting}_at` and **`Ply_Nav_LocoCamera_at`** (camera motion is
animation-driven). Blink clips: `Powers_Cast_Blink_In`, `_Loop`, `_Out`, and `Generic_Powers_Cast_Blink_Travel`. Locomotion
clips include walk, run, sprint, crouch and sneak variants, jump and land, slide (in, loop, out), mantle low, medium and high
(in normal, crouch and sneak variants), and swim in four directions. Export names are `AnimSequence_N`; the real name is the
`SequenceName` property.

**UELib caveats on Dishonored.** Class decompile lists only the first variable (the children chain is cut short), so use
`obj list` to enumerate members. Some enum-valued struct properties (`m_UsePowerAction`) and some untyped arrays fail to
decode.

## 5. Blink: behaviour specification

This is the behaviour `crates/dis_motion` implements, written as a specification in our own words. Names are the
game's reflected property names; values come from the tweak and INI files read at runtime.

### Data actually used at runtime

On every cast start, the component points `m_pPowerAttributes` at `Twk_Blink.m_Levels[CurrentLevel]`. That is a 3-slot
array of `PowerAttributes_Blink`. The working values are then copied from it:

- `m_fBlinkDistanceMax = Distance * (1 + mod)`, `maxHoriz = HorizDistance * (1 + mod)`, `maxVert = VertDistance * (1 + mod)`.
  `mod` comes from a game attribute lookup, so it's a percentage bonus. Treat it as 0 for a port.
- `stepDistance = DefaultBlinkStepDistance` and `stepInterval = DefaultTimeBetweenBlinkSteps`.

The per-level struct order is: Distance, HorizDistance, VertDistance, StepDistance, StepInterval, BendTimeDown,
BendTimeUp, WarmupTime, CooldownTime, WarmupWobbleMax, WarmupWobblePerSecond, WarmupDistortionMin, MoveDistortionMax,
MoveBlurMax, MoveReachMaxAtPct, CooldownWobbleCount (int), then the warmup, blink and fizzle sound events.
Tweak-wide fields used by the logic: `m_TargetTestExtent` (box half-extents for target sweeps), `m_fCloseCollisionDistance`,
`m_fCloseCollisionOffsetStep`, `m_fFallThreshold`, `m_fGroundMeshHeight`, and the flags `m_bLimitVerticalDistanceFromGround`
and `m_bTargetThroughAwarePawns`.

### Modes

`m_BlinkMode`: **0 = targeting** (held, warming up), **1 = travelling**, **2 = cooldown after a normal end**,
**3 = cooldown after an abort**. Every tick adds `dt` to `timeElapsed` and `timeInMode`, then runs the current mode.

### Casting conditions

The cast is refused unless the owner's physics mode is one of None, Walking, Falling, Swimming, Ladder, NavMeshWalking,
or Custom. Blinking from mid-air is legal.

### Targeting (mode 0, run every tick while held)

```
aim       = camera rotation as a unit vector; start = camera location
range     = blink_range(aim)                       # see below
end       = start + aim * range
hit       = box_sweep(start -> end, TargetTestExtent)
if hit started inside geometry: retry the sweep from the pawn's eye position
if hit and |hit - start| < CloseCollisionDistance:
    # wall right in front of you: nudge the ray off the wall, up first, then sideways in both directions,
    # by CloseCollisionOffsetStep each, keep the first offset whose sweep is clear, and re-sweep to `end`
point     = hit ? hit.location : end
goalNormal= hit ? hit.normal   : 0
if hit actor is a Pawn: set m_bStopAtPawns
back      = pull_back(point, aim)                  # see below
target    = point - aim * back
ground    = sweep(target -> target - (0,0,20000), TargetTestExtent)
if ground hit:
    groundPoint, groundNormal = ground.location, ground.normal
    # make sure the pawn's full collision height fits above the ground
    target.z += max(0, pawnCollisionHeight - (target.z - ground.z))
else: groundPoint = target, groundNormal = up
```

`blink_range(aim)` produces the "squashed sphere":
- Looking **level or down** (`aim.z <= 0`): `range = maxHoriz`.
- Looking **up**: scale the aim so its vertical part reaches `maxVert`, but never exceed `maxHoriz`:
  `s = min(maxVert / aim.z, maxHoriz)`, `v = aim * s`. If `LimitVerticalDistanceFromGround` is set, `v.z` is also capped at
  `maxVert - heightAboveGround`, where `heightAboveGround` comes from a downward sweep of length `maxVert` under the pawn.
  So the higher you already are, the less you can blink upward. `range = |v|`.

`pull_back(point, aim)` walks backwards from the hit point along `-aim` in steps of half the pawn's collision radius.
At each step it does tiny ±1-unit sweeps with the pawn's collision extent. It stops at the first spot where the pawn fits,
and never goes further back than the camera. This is why blinking at a wall puts you just in front of it.

The display places one of three markers: stand, ground or mantle. Stand vs ground is decided by the target's height
above the ground point against `GroundMeshHeight`, and a mantle marker is used when a ledge is targeted. The marker's
fall particle is low or high depending on the drop against `FallThreshold`.

### Release (start travelling)

1. Abort to mode 3 if the physics mode no longer allows blinking, or if the target is within one collision radius.
2. Snapshot the owner's rotation, location, velocity and physics mode.
   `maxDistanceFromOrigin = |target - origin|`.
3. **Switch physics to Flying**, so there's no gravity during travel.
4. **Auto-crouch.** If the pawn can crouch, crouch it and adjust Z to keep its feet in place, then set
   `m_bTriedCrouch`. This is how blink gets you through low gaps.
5. Play the cast sound and effects, set mode 1, and reset the step timer to `stepInterval`.

### Travel (mode 1)

```
stepTimer -= dt;  stepProgress = clamp(1 - stepTimer/stepInterval, 0, 1)   # -> BlinkStepProgress
if stepTimer <= 0:
    stepTimer += stepInterval
    to = target - location
    if dot(facing, to) <= 0: done                    # target is behind us
    else:
        stepGoal = |to| >= stepDistance ? location + normalize(to) * stepDistance : target
        sub_move(stepGoal)
    lastStepLocalDir = owner-space(moved delta)       # -> BlinkLocalDirection (feeds anim/lens)
    if |moved| < 2: done                             # stuck
    stepsTaken += 1
if horizontal |target - location| < 25: reachedTarget
if |location - origin| > maxDistanceFromOrigin or done or reachedTarget: end_blink(normal)
```

`sub_move(goal)` loops in **sub-steps of at most 50 units**. For each one:
- Set the pawn's velocity to `dir * stepDistance / stepInterval`. This is cosmetic: it gives animation and effects a speed.
  With no interval it uses 1000.
- Sweep **3× the sub-step ahead** with the pawn's extent:
  - If it hits a pawn that isn't the one being carried, touch or knock it and end.
  - If it hits a blink-blocking volume that contains the target, end.
- Otherwise **move without a collision check**. Safety comes from the targeting sweeps.
- Stop when the remaining distance grows (overshoot) or when the horizontal distance is under 25.
- If a pawn was touched and the target was a pawn, end early.

### End (normal) and cooldown

- Restore the pre-blink **velocity**, which is why blink keeps momentum (blink-jumps), and restore the pre-blink physics mode.
- Play the cooldown camera-lens effect.
- Then the **post-blink ledge check**: if the player's ledge detector has a valid ledge, run a small **step-up mantle** when
  the ledge height is within the step-up limit, otherwise a full **mantle**. With no ledge, try to **uncrouch**, and stay
  crouched if there's no headroom.
- Mode 2 (or 3 on abort). The cooldown timer counts up to `CooldownTime`, then the component deactivates.
- An abort during targeting plays the fizzle sound.
- If the configured impulse radius and strength are both > 0, a radial impulse is applied at the landing point.
  The shipped INI sets the strength to 0, so this is off.

### Screen effect (computed every active tick, written to the player's post-process settings)

- `distPct = clamp(|origin - location| / m_fBlinkDistanceMax, 0, 1)` and `smooth(t) = t*t*(3 - 2t)`.
- **Targeting:** wobble `= 0.5 * (sin(2π * WarmupWobblePerSecond * timeElapsed/WarmupTime - π/2) + 1)`. It is blended
  from `WarmupDistortionMin` toward full by `smooth(min(timeElapsed/WarmupTime², 1))`, then scaled by `WarmupWobbleMax`.
- **Travel:** distortion ramps from the warmup value toward `MoveDistortionMax` by `smooth(distPct / MoveReachMaxAtPct)`,
  and blur `= MoveBlurMax * distPct`.
- **Cooldown:** a decaying sine with `CooldownWobbleCount` cycles over `CooldownTime`.

Parameter names the materials read: `BlinkDistancePercentage`, `BlinkStepProgress`, `BlinkLocalDirection`,
`BlinkTimeElapsed`, `BlinkWarmupTime`, `BlinkCooldownTime`, `BlinkStepsTaken`, `BlinkLensIntensity`, `Blink_opacity`, `BlinkTargeting`.

### Still open for Blink
- Which of the 3 `m_Levels` slots each Blink tier uses. For the passive powers (§5e) the slot is the owned level and only that
  level's modifiers apply; Blink's own tweak levels are likely chosen the same way.
- The exact sideways axes used by the close-collision nudge (world Y versus aim-relative).
- The ledge detector (step-up mantle and full mantle) belongs to the movement work below.

## 5b. Drop assassination: behaviour specification (2026-10-09)

Falling onto a character and attacking kills it from above. Blink makes this the usual stealth kill: blink above or
beside a guard, fall, attack. This is the behaviour `crates/dis_motion` implements (`takedown.rs`), written as a
specification in our own words. Names are the game's reflected property names; values are read at runtime.

### Data used at runtime

- **Corvo's sword tweak**, class `DisTweaks_DropAssassinate`, under `Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo` in
  `Startup.upk`. It falls back to the shared sword tweak (`Twk_Inv_SwordBase`), then to the class defaults
  (`Default__DisTweaks_DropAssassinate` in `DishonoredGame.upk`). Missing fields are 0, as UE3 omits zero values.
  Fields: `m_fHitWindowInSeconds` (how far ahead the fall is predicted, 4 s for Corvo), `m_fMinDropDistToTarget` (0),
  `m_fMaxDropDistToTarget` (3.5 m), `m_fMaxAllowedDropJumpVel` (0), `m_fMinDropDownVel` (1 m/s).
- `m_DropAssassinateMoveSets[0]` is the human set: one linked action per side (`m_Drop_Front`, `_Left`, `_Right`,
  `_Back`), pairing Corvo's `Assassinate_Drop_<Side>` with the victim's `DisNPCAnim_AssassinationDrop`, with
  `m_bSlaveMovesMaster` set: the victim's animation places Corvo. Set `[1]` is the Tallboy's, with direction constraints.
- **Where Corvo lands**: the victim's half of each pair, `Generic_Assassination_Drop<Side>_Slave`, carries an
  `anchor_jnt`. At the clip's first frame it sits 50 cm in front of the victim, 80 cm to its left or right, or 60 cm
  behind it, level with its feet. These clips are with the City Watch's animations (`Npc_CityGuard_Generic_as`) in the
  mission packages, for example `L_Streets1_P.upk`. They are read from there at runtime.
- **How long Corvo is held**: his `Sword_Ready_Assassination_Drop<Side>_Master` clips (2.23 s) release him at their
  `DisNotify_AnimStateUnlock` notify, 1.59 to 1.84 s in depending on the side.

### When a target is looked for

Only while the player is falling (the falling state), every tick: for the on-screen prompt, and when attack is pressed.

1. **Predict the fall** over the hit window `T`: `vz' = min(vz, -m_fMinDropDownVel)`, and the displacement is
   `d = (vx·T, vy·T, vz'·T + ½·g·T²)`, with `g` the gravity and no horizontal acceleration.
2. **Sweep the player's own collision box** from its centre to centre + `d`. If that hits nothing, sweep it straight down
   by `d.z` instead. The first thing hit must be a character, which is then checked as below. A target that was already
   found is kept while it still passes.

### Checking a target

The target must be alive and not ragdolled. Then all of these must hold:

- the player's vertical speed is below `m_fMaxAllowedDropJumpVel` (0, so the player must be falling, not rising);
- the predicted drop `-d.z` is more than `m_fMinDropDistToTarget`;
- the height `h` from the player's feet down to the target's torso (its torso hit region) is more than `m_fMinDropDistToTarget`;
- sweeping the player's box from its centre to the target's centre hits the target first.

It is then **in range** if `h <= m_fMaxDropDistToTarget`, and otherwise a target to **track**. Awareness plays no part:
alerted characters can be dropped on too.

### Attacking

- **In range**: the kill starts now.
- **Track**: the player locks on. Horizontal velocity is cleared and downward speed doubled (`vz = min(vz, 0) · 2`), and
  steering, jumping, crouching and powers are disabled. Each tick the target is checked again. If it stops passing, the
  lock is dropped and the fall carries on as normal. Once it is in range, the kill starts.

### The kill

- **Side**: the player's position in the target's frame (x forward, y right). If `|x| <= |y|` it is Right when `y > 0`,
  else Left. Otherwise it is Front when `x >= 0`, else Back. Move sets list sides in the cardinal-direction order
  (forward, left, right, back).
- **Placement**: the player is moved to that side's anchor, on the victim's floor and facing it, and stands up.
  Velocity is cleared; the kill replaces the landing.
- **Animation**: Corvo plays `Sword_Ready_Assassination_Drop<Side>_Master`. Its `camera_jnt` carries the view: from
  just above eye height down into a kneel about 50 cm off the ground over the body, then back up. The arms are posed
  around that bone, so the world view follows it too. The clip posts its own sounds (`AnimNotify_AkEvent`: sword,
  impact, landing, cloth), a blood lens effect (`AnimNotify_CameraEffect`) and a `DishonoredNotify_CameraPitchTarget`.
- The player is released at the clip's unlock notify.

### Host side

Characters belong to the host. A sweep reports them as pawn hits, and `World::pawn` gives each one's centre, floor,
torso height and facing. `BoxWorld` boxes have no skeleton, so their torso is taken at 70% of their height.

### Still open for the drop assassination
- What `DishonoredNotify_CameraPitchTarget` (start and target pitch) does to the control rotation; the demo leaves the view
  level when the kill ends.
- Whether a wall at the landing anchor is handled specially. The kit stops the player short of it.
- The Tallboy's direction constraints, the adrenaline the kill grants (`m_fAdrenalineMultOnHit`) and the context-range
  scale (`m_fRayScalePercent`), none of which the human drop kill needs.

## 5c. Sword attack: behaviour specification (2026-10-09)

Corvo's basic sword swings. This is the behaviour `crates/dis_motion` implements (`melee.rs`), written as a specification
in our own words. Names are the game's reflected property names; values are read at runtime.

### Data used at runtime

- **Corvo's sword tweak**, class `DisTweaks_MeleeAttackPlayer`, under `Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo` in
  `Startup.upk`. It falls back to the shared sword tweak (`Twk_Inv_SwordBase`) and then the class defaults of
  `DisTweaks_MeleeAttackPlayer`, `DisTweaks_MeleeAttack` and `DisTweaks_ItemContext` (`DishonoredGame.upk`). Fields used:
  `m_fMaxContextRange` (2.1 m reach), `m_fRayScalePercent` (0.5), `m_fSweepingAttackSize` (60 cm),
  `m_fCrosshairAttackSize` (15 cm), `m_fMaxChainAttackTime` (1 s), `m_fAttackDamageMultiplier` (1) and
  `m_fCamShake_OnHitEnv` (200).
- **Reach by speed**, on the player pawn (`Default__DishonoredPlayerPawn`): `m_fMinSpeedRayScale` (4 m/s),
  `m_fRaySpeedScale` (0.005 per uu/s) and `m_fRaySpeedScale_Max` (1.5).
- **Damage**: the sword's `m_MeleeDamage` attribute (`DisTweaks_InventoryItem_Attributes` under the same sword), 10 at every
  difficulty. A character with no tweak of its own has `Twk_Pawn_DefaultNPC`'s `m_HealthMax` for the difficulty, 25.
- **The swings**, by the actions' animation states: forehand `RightAttackA/B` (`Sword_Ready_AttackRight_{A,B}_Small`),
  backhand `LeftAttackA/B` (`Sword_Ready_AttackLeft_..._Small`), sneak `SneakAttackA` (`Sword_Sneak_Attack_Small`), and the
  killing blows (`m_ForehandAttacks_Impact` / `m_BackhandAttacks_Impact`, states `Fatality_GenericB` / `_GenericA`:
  `Sword_Ready_Fatality_Generic_Right_A` / `_Left_A`). When the blade strikes the world the melee tree switches to its
  environment-hit sequence: the swing's `..._Big` clip, `..._BigChain` for a chained swing, `Sword_Sneak_Attack_BigWall`
  for the sneak attack.
- **Timing** comes from each clip's notifies, at its play rate (`RateScale`): `DishonoredNotify_AttackZone` (when the blade
  can hit; `m_fDamageZoneMaxTime`, 0.05 s, for how long), `DishonoredNotify_ChainInput` (from here a press is kept for the
  next swing), `DishonoredNotify_AttackInterruptable` (from here the next swing may start) and
  `DishonoredNotify_AnimStateExit` (the swing is over).

### Choosing the swing

On an attack press with no swing under way, or once the current one is interruptible (a press made from its chain input
on is kept and starts the next swing as soon as it is):

- The swing is **chained** if the player isn't sneaking (crouched) and the last swing started less than
  `m_fMaxChainAttackTime` ago, or the press was kept from the last swing.
- A swing that isn't chained, or that follows a backhand, is a **forehand**; one that is chained after a forehand is a
  **backhand**. So a chain alternates forehand, backhand, forehand.
- On the forehand side: a **killing blow** if the targeted character's health is no more than the damage; otherwise the
  **sneak attack** when sneaking; otherwise the forehand. On the backhand side: a killing blow or the backhand.
- Which of a swing's variants (A or B) plays is random.
- The game also has synced short finishers (`m_ForehandKill_Synced`, `m_fChanceOfSyncedAttack`), which need the victim's
  paired animation. Without one the killing blow is the plain impact swing.

### What the blade hits

- **Reach**: `m_fMaxContextRange · ((f − 1) · m_fRayScalePercent + 1)`, where `f` is 1, or for forward speed `v` above
  `m_fMinSpeedRayScale`, `1 + min((v − m_fMinSpeedRayScale) · m_fRaySpeedScale, m_fRaySpeedScale_Max)`. Sprinting at
  6 m/s reaches 3.15 m.
- **The target** (for the killing-blow choice and the HUD): a box of half-size `(m_fCrosshairAttackSize,
  m_fCrosshairAttackSize, 10)` swept from the camera along the view for the reach. If it meets the world first, a line
  along the view gets a second chance at a character.
- **The blow**: during the attack zone, a box of half-size `(m_fSweepingAttackSize, m_fSweepingAttackSize, 30)` is swept
  from the camera along the view for the reach. The game gathers every character it passes before the first piece of
  world geometry, puts the crosshair target first and orders the rest by the swing's direction. A world hit with no
  character before it is an **environment hit**: the swing switches to its recoil clip and the camera shakes
  (`m_fCamShake_OnHitEnv`).
- The kit lets the player keep moving and looking while swinging. The reach bonus for forward speed implies swings
  happen on the move, but the exact movement rules during a swing aren't confirmed.

### Host side

The kit reports the swing (`Swing`), a hit (`Hit`: target, damage, and whether it kills) and an environment hit
(`EnvHit`: point, shake, recoil clip). The host applies the damage. `World::pawn` gives each character's health. The kit's
`World` reports one hit per sweep, so it takes the first character the blade reaches.

### Still open for the sword
- The order the game sorts several characters in the sweep by (per swing direction), and blows that cut through more than
  one.
- Blocking, parrying, sword locks (`m_BigVersus_Action`, `m_fVersusAngle`) and the `Attack_BigHit` reaction need armed
  opponents.
- Impact sounds, sparks and blood come from the game's contact system (`DisContactType_Sword`), which isn't read yet.
- The auto look up and down (`m_bDoAutoLookUp`, `m_bDoAutoLookDown`) and the off-hand swipe the power hand plays
  (`m_OffhandSwipeForehand`).

## 5d. Ground assassination: behaviour specification (2026-10-09)

Attacking a character that hasn't noticed the player kills it outright: the stealth kill from behind (or from any side,
as long as it hasn't seen you). This is the behaviour `crates/dis_motion` implements (`takedown.rs`), written as a
specification in our own words.

### Data used at runtime

- **Corvo's sword tweak**, class `DisTweaks_Assassinate` under `Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo` in
  `Startup.upk`, over the shared sword tweak and the class defaults of `DisTweaks_ItemContext`, `DisTweaks_MeleeAttack`,
  `DisTweaks_MeleeAttackPlayer`, `DisTweaks_Finisher` and `DisTweaks_Assassinate`. Fields: `m_fMaxContextRange` (2.75 m),
  `m_fRayScalePercent` (1.25), `m_Assassination_Generic_ProbeExtents` (20, 20, 40), `m_AssassinateOnAwareness`,
  `m_bCanAssassinateRunners` (false), and from `DisTweaks_Finisher` `m_NumFinishersBeforeSlow_Min/Max` (4, 8) and
  `m_fTimeBeforeSlow_Min/Max` (90 s, 120 s).
- **Awareness** follows the game's `EAIAwareness` order: Unaware, AwareOfPlayer, Surprised, Suspicious, Fearful,
  InCombat, Begging, Choked. `m_AssassinateOnAwareness[i].m_bAssassinate` says which allow it. For Corvo they are
  Unaware, Surprised, Suspicious and Begging.
- **The kills**: `m_AssassinateMoveSets[0]` (humans) pairs Corvo's `Sword_Ready_Assassination_<Side>_Master` (slow) and
  `..._Fast<Side>_Master` (fast) with the victim's `Generic_Assassination_<Side>_Slave` / `..._Fast<Side>_Slave`, plus a
  plain `Sword_Ready_Assassination_Generic` with no victim animation. The victims' `anchor_jnt` at the first frame puts
  Corvo 1.0 to 1.2 m from the victim (slow front 1.2 m, back 1.0 m, sides 1.0 m; fast ones 1.2 m), and Corvo's clips
  release him at their `DisNotify_AnimStateUnlock` (about 1.7 to 2.0 s slow, 0.9 to 1.1 s fast, 0.35 s plain).

### When it is available

- **The target** is the character under the crosshair, found as for the sword (§5c) but with the assassination's reach:
  `m_fMaxContextRange`, with `m_fRayScalePercent` of the forward-speed bonus.
- It may be assassinated when its awareness allows it and it isn't running (unless `m_bCanAssassinateRunners`). The game also allows
  it in a few other states (apparently a stunned character); the kit leaves those out. **There is no angle test**: an unaware character can be killed from the front too.
- While it is available, the attack button assassinates instead of swinging.

### The kill

- **Side**: the player's position in the target's frame, as for the drop assassination (§5b).
- **Slow or fast**: the slow kill is the special one. It plays when the player isn't in a fight and either no fast kills
  are left, or the slow timer has run out and at least one fast kill has been seen. After a slow kill, the number of
  fast kills is a random count in `m_NumFinishersBeforeSlow_Min..Max` and the timer a random time in
  `m_fTimeBeforeSlow_Min..Max`. After a fast kill, the count goes down by one. The first kill is slow.
- **Plain kill**: if a box of `m_Assassination_Generic_ProbeExtents`, swept from the player to the target's torso through
  world geometry only, is blocked, or the player is falling, the plain kill is used and the victim stays put.
- **Placement**: the player is not moved. Unlike the drop kill, the victim is the one placed: it is turned and moved so
  that its anchor lies on the player, which puts it in front of the player at the anchor's distance. The player stands
  up, and is held until the clip's unlock notify, with the view following the clip's `camera_jnt` (as in §5b).

### Still open for the ground assassination
- The bend-time variants (`m_*_BendTimeFrozen_Aware`), the special and dramatic kills of story characters, and the
  versions while carrying a body (`..._CarryCorpse_Master`).
- The kit takes "in a fight" (which forces the fast kill) to be never; a host with AI should feed it.

## 5e. Agility: behaviour specification (2026-10-09)

Agility (the game's passive power `Celerity`) makes Corvo jump much higher, survive harder landings and, at its second
level, move faster. This is the behaviour `crates/dis_motion` (the power jump, `controller.rs`) and `crates/dis_data`
(`powers.rs`, the levels) implement, written as a specification in our own words. Values are read at runtime.

### Data used at runtime

- **The power list**, `DefaultPlayer.ini`, `[DishonoredGame.DishonoredPowersComponent]`, one `m_Powers=(...)` line per
  passive power: `m_Name`, and `m_Levels`, each with `m_RuneCost` and `m_Modifiers`. A modifier names an attribute
  (`m_AttributeName="Attribute_<Name>"`, the field `m_<Name>` of Corvo's attribute tweak), a type (`m_ModType`: `AddVal`,
  `AddBasePercent` or `SetVal`) and a value (`m_fModValue`). Agility has three levels: `[0]` costs nothing and changes
  nothing, `[1]` is Agility I and `[2]` is Agility II. `m_CurrentLevel=-1` means not owned.
- **Corvo's attributes** (§6): `m_JumpZ_PowerJump` and the four `..._PROTOTYPE` jump attributes are left at the class
  defaults (`Default__DisTweaks_PlayerPawn_Attributes`), zero, so without Agility there is no power jump.
- **The jump style**, `DefaultPlayerState.ini`, `[DishonoredGame.StatePlayerMasterJump] m_JumpStyle`. The install uses
  `eDisJumpStyle_FixedNormalJump_FixedPowerJump`. `m_fPowerJumpFOV` is 0, every `m_PowerJumpPostProcess` override is off and
  no `m_pPowerJumpSoundEvent` is set, so the power jump has no screen effect or sound of its own.

### Levels and modifiers

- Owning a power at a level applies **that level's modifiers only**. Changing level removes the old level's modifiers
  and adds the new level's: levels replace each other, they don't stack. This is why Agility II repeats Agility I's jump
  and fall-damage modifiers.
- An attribute's value is its base (Corvo's value for the difficulty) with its modifiers applied in order: `AddVal`
  adds the value, `AddBasePercent` adds that percentage of the base, `SetVal` replaces the value.
- Agility I: `JumpZ_PowerJump` +1300, fall damage and fall death at 25.5 and 35 m/s, and the prototype jump attributes.
  Agility II: the same, plus `GroundSpeedSprint` +30%, `WaterSpeed` +10%, `LandAnimRate` and `MantleAnimRate` +50%, and
  the sprint strafe and backward multipliers set to 0.5 and 0.4.

### The jump

A jump takes off at `m_JumpZ` (or, for the held style below, already as a power jump). While it rises the player is in
the jump state, which ends at the top of the jump (upward speed at or below 0.0001 uu/s): then the player is falling.

- **Fixed power jump** (the game's setting). On the way up, the time jump is held is counted; letting go resets it to 0.
  At the top, if the count is above 0.05 s and the power jump is available (`JumpZ_PowerJump` above zero) and hasn't
  been used in this jump, the upward speed is **set** to `JumpZ_PowerJump` and the jump carries on rising. At the next
  top it ends. So: hold jump through the jump and Corvo is kicked up a second time.
- **Held power jump with full stop** (`..._HeldPowerJump_FullStop`, not used by the install). Available when
  `HeldPowerJumpButtonTime_PROTOTYPE` is above zero: the jump takes off at `PowerJumpFullStop_PROTOTYPE`, and while jump
  isn't held the upward speed is capped at `FullStop_ExtraStopVel_PROTOTYPE`.
- **Continuous power jump** (`..._ContinuousPowerJump`, not used by the install). Available when
  `PowerJumpFullStop_PROTOTYPE` is above zero: from a normal take-off, while jump stays held the player is pushed up at
  `HeldPowerJumpAccel_PROTOTYPE` until it has been held longer than `HeldPowerJumpButtonTime_PROTOTYPE`. Letting go ends
  the push for this jump.

### Host side

`GameData::with_power(AGILITY, level)` applies a level to the loaded data (call it on the data as loaded), and
`MotionTuning::from_game` carries the jump into `MotionTuning::power_jump`. `StepEvents::power_jumped` reports the kick.

### Still open for Agility
- Whether the power jump also triggers controller rumble or a camera effect (there's no FOV change in the shipped
  settings), and whether jumping while carrying a body or from a moving base changes it.

## 6. Player motion: where the data lives (2026-10-07)

All of this is read at runtime by `crates/dis_data`. Values are not repeated here.

**Corvo's tweak tree is in `Startup.upk`.** The root is `Twk_Pawn_Corvo.Twk_Pawn_Corvo_Release` (class
`DisTweaks_PlayerPawn`). It references sub-tweaks by object reference:
- `m_pAttributeTweaks[0..3]` gives one `DisTweaks_PlayerPawn_Attributes` per difficulty (Easy, Normal, Hard, VeryHard).
  Every attribute is a `DisAttribute` struct `{m_fBaseValue1_Easy .. m_fBaseValue4_VeryHard}`. Motion attributes:
  `m_GroundSpeed`, `m_GroundSpeedSprint`, `m_GroundSpeedCrouch`, `m_GroundSpeedWalk`, `m_GroundSpeedSlowWalk`,
  `m_WaterSpeed`, `m_AccelerationRate`, `m_GroundStrafeMultiplier{Run,Sneak,Sprint}`,
  `m_GroundBackwardMultiplier{Run,Sneak,Sprint}`, `m_JumpZ`, `m_MaxSpeedBeforeFalling{Damage,Death}`,
  `m_LandAnimRate` and `m_MantleAnimRate`.
- `m_pMantleTweaks` and **`m_pMantleBlinkTweaks`** (a separate ledge-finder configuration used after a blink), class
  `DisTweaks_PlayerPawn_Mantle`. Fields: line-check step, min and max edge height, low and medium thresholds, the
  step-up option for low mantles, edge-face and slope angle limits, search distance, forward move, ledge-grab fall speeds.
- `m_pCameraTweaks` holds the min and max view pitch.
- The root object itself holds `m_fAirControl`, `m_fJumpImpulse`, `m_fSlideTime`, `m_fSlidePercentNotCancelable`,
  `m_bSlideAllowReturnToSprint`, `m_fAutoCrouchTestDistance`, `m_fAutoCrouchMinCrawlDepth` and `m_fClimbingSpeed`.

Other objects under `Twk_Pawn_DefaultPlayer` are fallback templates (`m_bOnlyUseAsFallback`); use the release tree instead.
UELib's `obj list` misses the `Twk_Pawn_Corvo` objects; our reader finds them.

**Pawn defaults (`DishonoredGame.upk`).** `Default__DishonoredPawn` and its `.CollisionCylinder` provide collision
half-height, radius, `CrouchHeight`, `CrouchRadius`, `MaxStepHeight`, `BaseEyeHeight`, `MaxFallSpeed` and `LadderSpeed`.
`Default__DishonoredPlayerPawn` overrides the radius values. From `Engine.upk`: `Default__Pawn.WalkableFloorZ`,
`Default__PhysicsVolume.GroundFriction` and `Default__WaterVolume.FluidFriction`.

**INI.** `DefaultGame.ini` sets `[Engine.WorldInfo] DefaultGravityZ`. `DefaultPlayerState.ini` sets the lean, swim, jump
style, falling stun velocity and walk crouch-FOV options. `DefaultCamera.ini` sets the default FOV, bob and roll amounts,
and the lean spring constants, angles, tilt and height ratio.

**Animation lengths (`Startup.upk`).** Each `AnimSequence` export under the `Ply_*` AnimSets carries `SequenceName`,
`SequenceLength` and `RateScale`. The demo paces mantle (low, medium, high, crouched variants), slide and landing from them.

**Struct defaults.** A cooked `ScriptStruct` export ends with its default values as a tagged stream. For example,
`DisTweaks_Blink.PowerAttributes_Blink` supplies the Blink fields that the per-level entries leave unset.

**Jump (`StatePlayerMasterJump`).** Z velocity is set from the `m_JumpZ` attribute (or the
carrying-corpse variant). The vertical velocity of the base the player stands on is added, then physics switches to Falling.
`m_JumpStyle` selects how the jump button lifts a jump further (Agility's power jump, §5e). `m_fJumpImpulse` is not the normal jump.

### Fidelity of the current motion core

| Part | Status |
|---|---|
| Blink targeting, range, pull-back, ground correction, stepping, end and cooldown, lens parameters | **Follows the game's behaviour** (§5), including moving without collision during travel |
| Jump velocity | **Follows the game** (attribute `m_JumpZ`) |
| All speeds, acceleration, air control, gravity, step height, collision sizes, mantle thresholds, slide timing, lean springs, swim | **Game values**, read at runtime |
| Walking and falling integration | UE3 `CalcVelocity`-style model; Dishonored's own "LocoNew" path not reproduced yet |
| Strafe and backward multipliers | Applied as a direction ellipse; the game's exact blend is not known |
| Mantle finder and mantle motion | Modelled from the tweak fields and animation lengths; the game's own edge finder is not reproduced. Ledges are world geometry only: characters are never mantled onto |
| Drop assassination: target search, lock-on dive, side, landing place and hold time | **Follows the game's behaviour** (§5b), with tweak values, anchors and timings read at runtime |
| Sword: swing choice and chaining, reach, target, blade sweep, environment hits, damage | **Follows the game's behaviour** (§5c); with several characters in one sweep the kit takes the first |
| Ground assassination: availability, side, slow/fast pacing, plain kill, victim placement, hold time | **Follows the game's behaviour** (§5d), with tweak values, anchors and timings read at runtime |
| Slide | Speed bleeds from entry speed to crouch speed over `m_fSlideTime`, with a cancel window. Modelled, not exact |
| Lean, bob, landing dip | Spring and procedural approximations; the game drives bob from `Ply_Nav_LocoCamera_at` (needs the Edge decoder) |
| Ladder, swim | Simple models using the game's speeds and accelerations |
| Agility: levels, modifiers, power jump (all three jump styles), fall limits, level II speeds | **Follows the game's behaviour** (§5e), with the power list and attributes read at runtime |

## 6b. Sound (Wwise), read at runtime (2026-10-07)

**Packages.** `CookedPCConsole/*.pck` are Wwise "AKPK" file packages, version 1. Header: `'AKPK'`, header size, version,
language-map size, banks-LUT size, stream-LUT size. **This revision has one extra u32 before the language map**, so the
banks LUT starts at `28 + langMapSize` and the stream LUT follows it. A LUT is a count followed by 20-byte entries
`{id, blockSize, size, startBlock, languageId}`, where `offset = startBlock * blockSize`.

**Soundbanks** are version 65 (Wwise 2011/2012). Sections are `BKHD`, `DIDX` (`{mediaId, offset, size}` into `DATA`),
`DATA`, `HIRC` and `STID`. `HIRC` holds `{u8 type, u32 size, u32 id, payload}` objects. Used here:
- **Event (4):** count, then action ids.
- **Action (3):** `u16` type (`0x04xx` = Play), `u32` target.
- **Sound (2):** source data starts with plugin id, stream type, then the media id.
- **Containers:** random/sequence (5), switch (6) and layer (9). Their `NodeBaseParams` begin with the override-FX flag,
  the FX count, the bus id and then the parent id. Children are recovered by matching sibling object ids in the payload
  after the parent field, which avoids version-specific property parsing.

Event ids are FNV-1 32-bit hashes of the lower-cased event name, and `AkEvent` objects in the UE3 packages carry those names.
Media are Wwise Vorbis (`fmt` codec `0xFFFF`). They are converted to Ogg with the `ww2ogg` crate (BSD-3, a port of hcs's ww2ogg).
`ww2ogg::validate` rejects loud impacts that legitimately clip, so the codebook set (standard or aoTuV) is chosen by fully
decoding both and keeping the one with the least clipping.

**Where the motion sounds are.**
- Footsteps per surface and gait: `Bank_Footsteps.pck`, events `FS_P_<Surface>_{Sn,R,Sp,Fall_Small,Fall_High}`, plus `FS_P_Slide_*`.
- Mantle, crouch, stand, fall wind, sprint breath and cloth: `Bank_Player.pck` (`Snd_P_*`).
- Blink: `Bank_Power_Player.pck`. The event names come from `Twk_Blink.m_Levels[n].m_p{BlinkWarmup,Blink,Fizzle}SoundEvent`
  (Start, Stop and Power_Empty).
- Swim: `Bank_UI_Ingame_Water.pck` (`Snd_P_Swim`).

## 6c. Particle effects (Cascade) and textures, read at runtime (2026-10-07)

**Systems.** `ParticleSystem` exports list `Emitters` (object refs). Each emitter has `LODLevels`; LOD 0 holds
`RequiredModule`, `SpawnModule`, an optional `TypeDataModule` (mesh emitters) and `Modules`. Every module is a
plain object whose curves are `RawDistributionFloat`/`RawDistributionVector` structs. **Cooked, the curve is baked**
into `LookupTable` (floats):
- `[0..2]` is the overall min/max.
- Samples follow, spaced `1/LookupTableTimeScale` apart in normalised time, starting at `LookupTableStartTime`.
- Each sample has `LookupTableNumElements` values; 2 means a min/max pair, picked per particle at random.
  `LookupTableChunkSize` floats per sample in all.
- A time scale of 0 means constant.

**Omitted values come from the class default.** UE3 serializes only what differs from the class default object, so
a module missing its `Rate` table, for example, uses `Engine.upk`'s `Default__ParticleModuleSpawn`. Structs are merged
field by field. Bursts are `BurstList`, an array of tagged `{Count, CountLow, Time}` structs, with `Time` in normalised
emitter time.

**Modules used by the motion effects:** Required (material, alignment `PSA_Velocity`, local space, kill on
deactivate, duration, loops, delay), Spawn (rate, rate scale, bursts), Lifetime, Size, SizeMultiplyLife,
SizeMultiplyVelocity, Color, ColorOverLife, ColorScaleOverLife, Velocity (with radial), VelocityOverLifetime,
Acceleration, Location, LocationPrimitiveCylinder, Rotation, RotationRate, RotationRateMultiplyLife, OrientationAxisLock,
TypeDataMesh, MeshRotation and MeshRotationRate.
Not handled yet: SubUV flipbooks, Orbit, Collision and VelocityInheritParent.

**Mesh particles (2026-10-08).** The static-mesh reader decodes cooked LOD 0 positions, half/full UVs, BGRA vertex
colours and 16-bit triangle indices. It skips collision data and section metadata. The renderer uses the original
geometry with each particle's three-axis size and rotation, including the emitter's pre-rotation and local-space
transform. No extracted geometry ships in the repository.

**Where the effects are.**
- Blink targeting markers: `Twk_Blink.m_pGroundPS`, `m_pLowFallPS`/`m_pHighFallPS` and `m_pMantlePS`, all in
  `Vfx_GamePlay.Blink.*`.
- Blink arrival lens effect: `m_pCooldownEffect` -> `DisTweaks_EmitterCameraLensEffect.m_DefaultParticleSystem`, which is
  `vrosier_TestFX2.Blink2.Blink_PlayerWind_01` (shipped despite the test package name). Lens effects sit
  `DistFromCamera` in front of the camera (`Default__EmitterCameraLensEffectBase`).
- Footsteps, slides, landings and water: `Vfx_PhysMat.*` (DishonoredGame.upk). Swimming and lens drips: `Vfx_Water.*` and
  `Vfx_PhysMat.Water.ps_camera_water` (Startup.upk).

**Materials.** Effects use `MaterialInstanceConstant`s over a few parent materials in `Vfx_Materials.main_materials`
(`Glow_PMAT`, `DefaultAdditive[_2Sided]_PMAT`, `DefaultTranslu_PMAT`, `WaterSplash_PMAT`). Texture parameters follow a
naming scheme: `D_Diffuse`, `O_Opacity` (masks packed per channel), `Shape`, `UV_Distortion`, `F_Fog_Texture`. We
approximate the material graph:
- the parent decides additive vs translucent;
- colour comes from the diffuse texture, coverage from the opacity or shape texture's red channel;
- glow materials are a radial glow modulated by their noise texture;
- the `Color` vector parameter tints.

Imported materials and textures are resolved by object path against the loaded packages. In particular, the hand's
`Startup.upk` effects import the Blink glow material from `DishonoredGame.upk`; caching a generic fallback under that
asset name gave both the hand and marker the wrong glow. An unresolved import now remains unresolved rather than
poisoning the shared material cache. Sprite textures use linear filtering. A depth-prepass particle shader fades
coverage over 2.5 cm at the hand and 10 cm in the world to soften intersections with opaque surfaces. These distances
and the compact radial glow profile are rendering approximations, not recovered shader constants.

**Textures.** After a `Texture2D`'s properties: an empty `SourceArt` bulk record, then the mip count, then per mip
`{flags, elementCount, sizeOnDisk, offset}`, the payload (inline unless flag `0x1`), and the mip's width and height.
Flag `0x1` puts the payload in `<TextureFileCacheName>.tfc` at `offset`. Flag `0x10` means LZO in the package chunk
format. Formats seen: DXT1, DXT5, ARGB8 and G8. `Textures.tfc` is 1.2 GB, so mips are read by byte range.

### Matching the Blink marker to the game (2026-10-07, compared against in-game screenshots)

- **Placement (the game's targeting display).** The *ground* system goes at the ground point found under the
  target, rotated to the pawn's yaw with **pitch -90°**. Its local X therefore points down: its glow streaks travel up
  (velocity -X), and its `-X` axis-locked cards lie flat on the floor. The *fall* system goes at the target point with
  a zero rotator (a zero-initialised global). It is hidden unless the target is more than 15 units above the ground
  point; since the target is the pawn centre, it normally shows. With a ledge, the mantle system replaces the ground one.
  `m_pStandMesh` and friends (the `BlinkTargetingGoal` mesh) are unset in the shipped tweak.
- **LOD matters.** Both systems have `LODDistances = [0, 256]`. Beyond 2.56 m the engine uses LOD 1, where the
  velocity-aligned glow-streak emitter spawns at 15/s (0 at LOD 0). Those streaks *are* the vertical beam. The runtime
  now loads every LOD level and picks one by camera distance.
- **Velocity alignment.** Sprite Y runs along the velocity and X across it. `SizeMultiplyVelocity` sets
  `size = base * speed * multiplier` per axis, with no clamping.
- **Square alignment.** `PSA_Square` uses the X size for both sprite axes, including axis-locked cards. Using the
  independent Y size stretched the rotating glow cards into the broad bars seen in the earlier demo.
- **Glow material parameters.** `G_GlowPower` sharpens the approximated radial falloff. `F_Fog_Intensity` scales brightness
  (25 on `Blink_Glow_02`). `G_GlowColor` multiplies `Color`. Translucent materials use their `Opacity` scalar (or
  `Color.a`). Diffuse textures with data only in red are masks, not red colour.
- **Travel screen effect.** The game uses a radial blur with a dark tunnel vignette. The demo now uses an embedded fullscreen lens pass driven by
  Blink's blur and distortion parameters: 12-tap weighted radial blur, peripheral lens warp, subtle chromatic
  separation and a tunnel vignette. It runs on the world camera, before the separate arms camera and UI,
  and bypasses the pass when inactive. HDR bloom adds a soft halo to the targeting particles. This is an
  original approximation of the look, not the game's post-process material.

## 6d. First-person arms, sword and animation (2026-10-07)

- **Where.** The arms are `Engine.upk` `Ply_Player.Skm_Player` (one material, `D_Diffuse` and `N_Normals` at 2048²
  only, so the reader decodes the top mip and box-halves it). The sword is `Startup.upk` `Wpn_PlySwords.Wpn_PlySword01`.
  The first-person `AnimSet`s (`Ply_*`) are in `Startup.upk`: 385 sequences decode in about 100 ms.
- **Skeletal mesh layout (v801/30), as far as LOD 0 needs.** After the tagged properties: user bounds (bone name,
  offset vector, radius), the bounds, the material list, `MeshOrigin`, `RotOrigin`, the Edge skeleton as a byte array,
  then the reference bones (name, flags, quaternion, position, child count, parent, colour). Per LOD: sections (material,
  chunk, first index, triangle count, one byte), a 16-bit index buffer, used bones, chunks (first vertex, rigid and soft
  vertex arrays that are empty when cooked, a chunk-local bone map, counts), sizes, required bones, a raw-points bulk
  record, then the GPU skin buffer. Its vertices are 8 bytes of packed tangent and normal, 4 bone indices (chunk-local),
  4 weights, a float position and half-float UVs.
- **Edge animation, as implemented in `crates/edge_anim`.** Little-endian header with a 16-bit header size at byte 12
  and 16-bit counts after it (joints, frames, frame sets, evaluation buffer, then constant and animated channel counts
  for rotation, translation, scale and user channels). The 32-bit offsets that follow are relative to the field that
  holds them. Channel tables (joint indices) sit after a fixed 96-byte header, padded to 8 bytes for constant rotations
  and 4 for the rest. Constant rotations are 48-bit packed quaternions (three components, the largest dropped).
  Animated channels are stored in frame sets: each has a base frame, an "intra" frame count and a DMA-style size and
  offset. Inside a set, keyframe values are bit-packed per channel with a per-channel packing spec (sign, exponent and
  mantissa widths), the bit stream is big-endian and MSB-first, and intra frames are flagged per channel. Joints are
  matched to the skeleton by name hash (the skeleton has hashes, parents and a base pose that equals the UE bind pose).
  Additive sequences add on top of the base pose. Facts came from the prose write-up in dishonoredrecompiled (see the
  provenance caveat in §2); no code was copied, and the decoder was checked by evaluating poses against the bind pose.
- **Conventions that took measurement.**
  - Bone quaternions are used as stored with glam's `q * v` (no W flip).
  - Rotator to matrix: `Rz(yaw) * Ry(-pitch) * Rx(-roll)`, 65536 units per turn.
  - The view is the `camera_jnt` bone: +X up, +Y right, -Z forward. The demo maps its +X to Bevy's +Y, +Y to +X.
  - The sword sits at `handAttachment_R_jnt` × socket `RightHandWpn` (rotation read from the install) × the sword's own
    `RotOrigin`, minus its `MeshOrigin`.
  - The arm FOV of 75 is vertical; measured against the screenshots, the hand placement only matches that way.
  - The left hand's `Power` socket (`handAttachment_L_jnt`) is where casting effects attach.
- **Hand effects come from animation notifies.** The `Ply_Powers_as` sequences carry `AnimNotify_PlayParticleEffect`s
  (in each sequence's `Notifies`: time, notify object). `Powers_Cast_Blink_In` and `Powers_Cast_Blink_Loop` start
  `Vfx_GamePlay.Powers.Ps_Tattoo_Glow_02` at 0 s, so it re-fires on every 0.33 s loop while targeting.
  `Powers_Cast_Blink_Out` starts `Ps_Tattoo_Glow_04` at 0.07 s. All of them attach to the `Tattoo` socket
  (`hand_L_jnt`, on the back of the hand) in the foreground group. Each is a translucent dirt-smoke emitter plus two
  additive gold `Blink_Glow_02` cards. The swim sequences put `Player_Swimming` on the middle fingers the same way.
  `dis_data::viewmodel` reads every particle notify generically, and the demo simulates the effects in view space.
- **Tattoo emission (2026-10-08).** The arm material supplies `D_Diffuse`, `D_Diffuse_No_Tatoo`, the blue power-hand
  region in `SP_SpecPower`, and `Power_Hand_Color`. The renderer derives an emissive mask from the diffuse difference,
  thresholds compression noise and gates it with that region. This keeps the gold emblem on the deforming skin.
  Brightness is smoothed between idle, targeting, travel and cooldown; that envelope approximates the game look.
- **Blink travel streaks.** `Twk_Blink.m_pCooldownEffect` is a camera-lens effect whose system is
  `vrosier_TestFX2.Blink2.Blink_PlayerWind_01`: 12 velocity-aligned glow streaks and 5 soft flashes thrown backwards
  past the camera. That's the white streaking in screenshots of the travel; the radial blur is the post-process
  (`m_fMoveBlurMaxStrength` 2.5). The demo's dedicated radial gather uses Blink's blur parameter, with a sharp
  centre and progressively longer streaks toward the edges (see §6c).
- **Layering in the demo.** A base layer (`Sword_Ready_*`, `Sword_Sneak_*`, `Sword_Slide*`, `Empty_Swim*`, mantle and
  landing) and a left-arm layer (`Powers_Idle/Walk/Sprint/Jump`, `Powers_Cast_Blink_In/Loop/Out`,
  `Generic_Powers_Cast_Blink_Travel`) masked to the joints `Powers_Idle` animates, crossfaded over 0.18 s. Skinning is
  done on the CPU and drawn by a second camera on its own render layer, so the arms never clip into walls.

- **Test-course dressing.** The demo textures its boxes with `DishonoredGame.upk` textures read at runtime
  (`grounds.street_cobbles_01`, `modular_rocks.modular_rock_01`, `wood_plank_01`, diffuse and normal). Unreal normal
  maps are Y-down, so green is flipped for Bevy.

## 7. Status and next steps

Done: `upk` (package, texture reader), `dis_data` (tuning, sound and effect loader), `dis_motion` (motion core and
Blink, Agility, the drop and ground assassinations, the sword, tests), `wwise` (sound packages), `cascade` (particle runtime), `edge_anim` (Edge animation decoder), and `sinhonor_demo` (Bevy course with Corvo's animated arms and sword, sounds,
the game's particle effects, sword fights and drop and ground assassinations on box guards, and `--autopilot` verification routes).

Next:
1. Match the game's mantle edge finder (`DishonoredMantleEdgeFinderComponent`) more closely,
   and the Slide and Leaning states. Replace the models above.
2. The other passive powers' motion effects, if any are wanted (Vitality, Bloodthirsty and Shadow Kill don't change motion).
3. Drive camera bob and mantle camera motion from the game's camera animations (the Edge decoder now exists).
4. SubUV flipbooks and original particle material scrolling/distortion; Blink's screen
   post-process material (`BlinkDistancePercentage` and the other parameters) instead of the approximate lens shader.
5. A host adapter example: dropping `dis_motion` into another Rust game, in the mashup style.
6. Sword polish: the contact system's impact sounds, sparks and blood, and the open questions in §5c.
7. Drop assassination polish: the clips' blood lens effect (`DisEmitterCameraLensEffect_blood_medium`), real victims (a
   guard mesh playing its `Generic_Assassination_Drop*_Slave` clip), and the open questions in §5b.

## 8. Credits

UELib / UE Explorer (Eliot van Uytfanghe), UE Viewer (Konstantin Nosov / Gildor), ue3-tools and dishonored-toolkit
(deadYokai), dishonoredrecompiled and dismod (ectrc), CodeRed-Generator (CodeRedModding), lzokay-native (MIT), [ww2ogg](https://github.com/coconutbird/ww2ogg-rs) Rust port of [hcs64/ww2ogg](https://github.com/hcs64/ww2ogg) (BSD-3-Clause), lewton (MIT/Apache-2.0). Cascade/LookupTable layout cross-checked against UE Viewer's UE3 notes (MIT).
Runtime dependencies: [Bevy](https://bevyengine.org) and [glam](https://github.com/bitshifter/glam-rs) (MIT OR Apache-2.0).
Structural references: [iw4L](https://github.com/vladtrc/iw4L), [gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust),
[benilla](https://github.com/samwhosung/benilla), [2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup).
[dishonored-bevy](https://github.com/Eamo5/dishonored-bevy) (Eamo5) pointed us to the paired kill animations and their
`anchor_jnt`. It has no licence, so it was consulted for facts only and no code from it is used.
