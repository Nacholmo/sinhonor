# sinhonor — research notes

**Scope:** a **portable Dishonored motion kit**, in the spirit of how
[2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) drops Skate 3 skating into IW4L.
It covers player motion (walk, sprint, crouch, jump, fall, slide, mantle, lean, swim, ladder climb, and the camera feel)
and the **motion powers**: Blink, plus Agility's power jump and fall-damage changes. All of it is packaged to be modded
into *other* games. This is *not* a full rewrite of Dishonored. AI, combat, non-motion powers, UI, saves, audio and the
campaign are out of scope. They appear below only where motion or Blink touches them (for example, blink knockdown damage types).

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

There is no public, clean-room spec of Dishonored's gameplay logic. The movement and Blink logic has to be learned from
the game's own UnrealScript bytecode, and from the native exe only where a function is `native`.

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
  The layout is described in prose in `dishonoredrecompiled/resources/docs/edgeanim.md`. A Rust decoder would need a clean
  implementation from that description.

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
- Native-side strings name the blink post-process parameters: `BlinkCooldownTime`, `BlinkDistancePercentage`,
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

**Native binary (`Dishonored.exe`).** It is a 32-bit PE with image base `0x400000`, a 2022 rebuild timestamp, and **no SteamStub
wrapper** (sections `.text .rdata .data .rsrc .reloc` only). It has MSVC RTTI. UE3 native thunks are registered through
`{const char* "U<Class>exec<Func>", fnptr}` pair tables in `.data`/`.rdata`. Walking those tables labels 2,560 natives.
Class registration strings are UTF-16 (`"UDishonoredActivePowerComponent_Blink"`).

**UELib caveats on Dishonored.** Class decompile lists only the first variable (the children chain is cut short), so use
`obj list` to enumerate members. Some enum-valued struct properties (`m_UsePowerAction`) and some untyped arrays fail to
decode.

## 5. Blink: native behaviour (from Ghidra, written in our own words)

How it was found: the class's static registration record in `.data` (size `0x168`, matching the 360-byte retail
layout) points at its internal constructor. That constructor installs the vtable, and diffing it against the parent
`DishonoredActivePowerComponent` vtable leaves 16 Blink overrides in `0xbe7000–0xbfd000`. Field offsets were matched to the
reflected property order: the Blink members start at `+0x90`, and the parent's `m_TargetPoint` is at `+0x60`.

### Data actually used at runtime

On every cast start, the component points `m_pPowerAttributes` at `Twk_Blink.m_Levels[CurrentLevel]`. That is a 3-slot
static array of `PowerAttributes_Blink`, `0x4c` bytes each. The working values are then copied from it:

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
- What exactly the 3 `m_Levels` slots map to (they're indexed by `CurrentLevel`; only two have rune costs).
- The exact sideways axes used by the close-collision nudge (world Y versus aim-relative).
- The ledge detector (`ad35e0` step-up mantle, `acd820` full mantle) belongs to the movement work below.

## 6. Suggested order

1. Rust UE3 package reader: header, LZO chunk decompression, name, import and export tables, tagged properties.
   Write our own and use the UELib (MIT) and ue3-tools docs as references.
2. Runtime INI reader for `Config/Default*.ini`.
3. Read the blink tweak and default objects and the player state defaults from the packages at runtime.
4. Behaviour (blink stepping and targeting, player states, LocoNew walking) comes from Ghidra on `Dishonored.exe` in
   `context/`. Write it up here as prose and pseudocode in our own words, then reimplement it. Decompiled output is never committed.
5. Edge animation decoder.

## 7. Credits

UELib / UE Explorer (Eliot van Uytfanghe), UE Viewer (Konstantin Nosov / Gildor), ue3-tools and dishonored-toolkit
(deadYokai), dishonoredrecompiled and dismod (ectrc), CodeRed-Generator (CodeRedModding), lzokay-native (MIT, local tooling), Ghidra (NSA, Apache-2.0).
Structural references: [iw4L](https://github.com/vladtrc/iw4L), [gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust),
[benilla](https://github.com/samwhosung/benilla), [2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup).
