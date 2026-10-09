//! Loads Dishonored's player-motion and Blink tuning from the user's own install, at runtime.
//!
//! Nothing here embeds game values: every number comes from the install's packages
//! (`Startup.upk`, `DishonoredGame.upk`) or its `Config/Default*.ini` files. If a value can't
//! be found, a neutral placeholder is used and a warning is recorded in [`GameData::warnings`].
//! See `NOTES.md` for where each value lives and how it was identified.

pub mod ini;
pub mod effects;
pub mod melee;
pub mod powers;
pub mod sounds;
pub mod surfaces;
pub mod takedown;
pub mod viewmodel;

use ini::Ini;
use std::path::{Path, PathBuf};
use upk::{lookup, ObjRef, Package, Property, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
    VeryHard,
}

impl Difficulty {
    fn index(self) -> usize {
        self as usize
    }
    fn field(self) -> &'static str {
        ["m_fBaseValue1_Easy", "m_fBaseValue2_Normal", "m_fBaseValue3_Hard", "m_fBaseValue4_VeryHard"][self.index()]
    }
}

/// Player movement tuning (Unreal units: 1 uu ~ 1 cm, Z up).
#[derive(Clone, Debug)]
pub struct PlayerTuning {
    pub gravity_z: f32,
    pub collision_radius: f32,
    pub collision_half_height: f32,
    pub crouch_radius: f32,
    pub crouch_half_height: f32,
    pub max_step_height: f32,
    pub base_eye_height: f32,
    pub max_fall_speed: f32,
    pub ladder_speed: f32,
    pub walkable_floor_z: f32,
    pub ground_friction: f32,
    pub water_friction: f32,

    pub ground_speed: f32,
    pub ground_speed_sprint: f32,
    pub ground_speed_crouch: f32,
    pub ground_speed_walk: f32,
    pub ground_speed_slow_walk: f32,
    pub water_speed: f32,
    pub accel_rate: f32,
    pub strafe_mult_run: f32,
    pub strafe_mult_sneak: f32,
    pub strafe_mult_sprint: f32,
    pub backward_mult_run: f32,
    pub backward_mult_sneak: f32,
    pub backward_mult_sprint: f32,
    pub jump_z_attribute: f32,
    /// How the jump button can lift a jump further (`StatePlayerMasterJump.m_JumpStyle`).
    pub jump_style: JumpStyle,
    /// The power jump's upward speed (`m_JumpZ_PowerJump`); Agility raises it from nothing.
    pub jump_z_power_jump: f32,
    /// For the held and continuous jump styles: `m_PowerJumpFullStop_PROTOTYPE`,
    /// `m_FullStop_ExtraStopVel_PROTOTYPE`, `m_HeldPowerJumpButtonTime_PROTOTYPE` and
    /// `m_HeldPowerJumpAccel_PROTOTYPE`.
    pub power_jump_full_stop: f32,
    pub full_stop_extra_stop_vel: f32,
    pub held_power_jump_button_time: f32,
    pub held_power_jump_accel: f32,
    pub jump_impulse: f32,
    pub air_control: f32,
    pub max_speed_before_fall_damage: f32,
    pub max_speed_before_fall_death: f32,
    pub land_anim_rate: f32,
    pub mantle_anim_rate: f32,

    pub slide_time: f32,
    pub slide_percent_not_cancelable: f32,
    pub slide_allow_return_to_sprint: bool,
    pub auto_crouch_test_distance: f32,
    pub auto_crouch_min_crawl_depth: f32,

    pub bob_amount: f32,
    pub roll_amount: f32,
    pub min_view_pitch: f32,
    pub max_view_pitch: f32,
    pub default_fov: f32,
    pub fov_blend_speed: f32,

    pub lean: LeanTuning,
    pub swim: SwimTuning,
    pub mantle: MantleTuning,
    /// Ledge finder used right after a blink (separate tweak object in the game).
    pub mantle_blink: MantleTuning,
    pub fall_stun_velocity: f32,
}

/// `StatePlayerMasterJump.eDisJumpStyle`, in the game's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JumpStyle {
    /// Holding jump to the top of a jump kicks the player up again (the game's setting).
    #[default]
    FixedPower,
    /// A higher jump from the start; letting go of jump cuts the climb.
    HeldPowerFullStop,
    /// Holding jump keeps pushing the player up for a while.
    ContinuousPower,
}

#[derive(Clone, Debug)]
pub struct LeanTuning {
    pub lean_speed: f32,
    pub release_time: f32,
    pub max_angle: f32,
    pub max_angle_crouched: f32,
    pub camera_tilt_percent: f32,
    pub height_pct: f32,
    pub springiness: f32,
    pub damping: f32,
}

#[derive(Clone, Debug)]
pub struct SwimTuning {
    pub min_accel: f32,
    pub max_accel: f32,
    pub max_speed_no_stroke: f32,
    pub stroke_time: f32,
    pub surface_stroke_delay: f32,
}

#[derive(Clone, Debug)]
pub struct MantleTuning {
    pub line_check_step: f32,
    pub min_edge_height: f32,
    pub max_edge_height: f32,
    pub low_max_edge_height: f32,
    pub medium_max_edge_height: f32,
    pub low_uses_step_up: bool,
    pub low_step_up_blend_time: f32,
    pub max_vertical_angle_edge_face: f32,
    pub max_horizontal_angle_edge_face: f32,
    pub max_slope_angle_edge_top: f32,
    pub edge_search_dist: f32,
    pub forward_move_amount: f32,
    pub fall_speed_for_ledge_grab: f32,
    pub max_fall_speed_for_mantle: f32,
    pub ledge_grab_min_edge_height: f32,
}

/// One Blink upgrade level (`DisTweaks_Blink.m_Levels[n]` merged over the struct defaults).
#[derive(Clone, Debug)]
pub struct BlinkLevel {
    pub distance: f32,
    pub horiz_distance: f32,
    pub vert_distance: f32,
    pub step_distance: f32,
    pub step_interval: f32,
    pub warmup_time: f32,
    pub cooldown_time: f32,
    pub warmup_wobble_max: f32,
    pub warmup_wobble_per_second: f32,
    pub warmup_distortion_min: f32,
    pub move_distortion_max: f32,
    pub move_blur_max: f32,
    pub move_reach_max_at_pct: f32,
    pub cooldown_wobble_count: i32,
}

#[derive(Clone, Debug)]
pub struct BlinkTuning {
    pub levels: Vec<BlinkLevel>,
    /// Wwise events the Blink tweak references (warmup, blink, fizzle), from the first level.
    pub sound_events: sounds::BlinkSoundEvents,
    pub target_test_extent: [f32; 3],
    pub close_collision_distance: f32,
    pub close_collision_offset_step: f32,
    pub fall_threshold: f32,
    pub ground_mesh_height: f32,
    pub limit_vertical_from_ground: bool,
    pub mana_cost: i32,
    pub impulse_radius: f32,
    pub impulse_strength: f32,
}

#[derive(Clone, Debug)]
pub struct GameData {
    pub install: PathBuf,
    pub difficulty: Difficulty,
    pub player: PlayerTuning,
    pub blink: BlinkTuning,
    pub drop_assassinate: takedown::DropAssassinateTuning,
    pub assassinate: takedown::AssassinateTuning,
    pub melee: melee::MeleeTuning,
    /// Passive powers and their levels (Agility is [`powers::AGILITY`]).
    pub powers: Vec<powers::Power>,
    /// Player animation lengths in seconds (`SequenceLength / RateScale`), keyed by sequence name,
    /// from the first-person `Ply_*` AnimSets in `Startup.upk`.
    pub anim_lengths: std::collections::HashMap<String, f32>,
    pub warnings: Vec<String>,
}

impl GameData {
    pub fn anim(&self, name: &str) -> Option<f32> {
        self.anim_lengths.get(name).copied()
    }

    pub fn power(&self, name: &str) -> Option<&powers::Power> {
        self.powers.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// This data with the passive power `name` owned at `level` (its index in the power's levels;
    /// for Agility 1 and 2 are the two upgrades). Call it on the data as loaded: a level's
    /// modifiers replace the previous level's rather than adding to them, as in the game.
    pub fn with_power(&self, name: &str, level: usize) -> GameData {
        let mut g = self.clone();
        if let Some(l) = self.power(name).and_then(|p| p.levels.get(level)) {
            g.player.apply(&l.modifiers);
        }
        g
    }
}

impl PlayerTuning {
    /// The motion attribute with the game's name (`Attribute_<name>` without the prefix).
    pub fn attribute_mut(&mut self, name: &str) -> Option<&mut f32> {
        Some(match name {
            "GroundSpeed" => &mut self.ground_speed,
            "GroundSpeedSprint" => &mut self.ground_speed_sprint,
            "GroundSpeedCrouch" => &mut self.ground_speed_crouch,
            "GroundSpeedWalk" => &mut self.ground_speed_walk,
            "GroundSpeedSlowWalk" => &mut self.ground_speed_slow_walk,
            "WaterSpeed" => &mut self.water_speed,
            "AccelerationRate" => &mut self.accel_rate,
            "GroundStrafeMultiplierRun" => &mut self.strafe_mult_run,
            "GroundStrafeMultiplierSneak" => &mut self.strafe_mult_sneak,
            "GroundStrafeMultiplierSprint" => &mut self.strafe_mult_sprint,
            "GroundBackwardMultiplierRun" => &mut self.backward_mult_run,
            "GroundBackwardMultiplierSneak" => &mut self.backward_mult_sneak,
            "GroundBackwardMultiplierSprint" => &mut self.backward_mult_sprint,
            "JumpZ" => &mut self.jump_z_attribute,
            "JumpZ_PowerJump" => &mut self.jump_z_power_jump,
            "PowerJumpFullStop_PROTOTYPE" => &mut self.power_jump_full_stop,
            "FullStop_ExtraStopVel_PROTOTYPE" => &mut self.full_stop_extra_stop_vel,
            "HeldPowerJumpButtonTime_PROTOTYPE" => &mut self.held_power_jump_button_time,
            "HeldPowerJumpAccel_PROTOTYPE" => &mut self.held_power_jump_accel,
            "MaxSpeedBeforeFallingDamage" => &mut self.max_speed_before_fall_damage,
            "MaxSpeedBeforeFallingDeath" => &mut self.max_speed_before_fall_death,
            "LandAnimRate" => &mut self.land_anim_rate,
            "MantleAnimRate" => &mut self.mantle_anim_rate,
            _ => return None,
        })
    }

    /// Applies attribute modifiers, each attribute's in order from its current value. Attributes
    /// that aren't about motion (health, mana...) are skipped.
    pub fn apply(&mut self, modifiers: &[powers::AttributeModifier]) {
        let mut done: Vec<&str> = Vec::new();
        for m in modifiers {
            if done.contains(&m.attribute.as_str()) {
                continue;
            }
            done.push(&m.attribute);
            if let Some(v) = self.attribute_mut(&m.attribute) {
                *v = powers::modified(*v, modifiers.iter().filter(|o| o.attribute == m.attribute));
            }
        }
    }
}

#[derive(Debug)]
pub enum Error {
    NotFound(PathBuf),
    Package(PathBuf, upk::Error),
    Missing(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound(p) => write!(f, "Dishonored install not found at {}", p.display()),
            Error::Package(p, e) => write!(f, "{}: {e}", p.display()),
            Error::Missing(s) => write!(f, "missing in install: {s}"),
        }
    }
}

impl std::error::Error for Error {}

/// Locates the install: explicit path, then `DISHONORED_DIR`, then common Steam libraries.
pub fn find_install(explicit: Option<&Path>) -> Option<PathBuf> {
    let looks_right = |p: &Path| p.join("DishonoredGame/CookedPCConsole/Startup.upk").is_file();
    if let Some(p) = explicit {
        return looks_right(p).then(|| p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("DISHONORED_DIR").map(PathBuf::from) {
        if looks_right(&p) {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let windows_steam = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Program Files (x86)"))
        .join("Steam");
    let steam_roots = [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
        windows_steam,
    ];
    let mut candidates: Vec<PathBuf> = steam_roots.iter().map(|r| r.join("steamapps/common/Dishonored")).collect();
    // Extra Steam libraries listed in libraryfolders.vdf (other drives on Windows).
    for vdf in steam_roots.iter().map(|r| r.join("steamapps/libraryfolders.vdf")) {
        if let Ok(text) = std::fs::read_to_string(vdf) {
            for line in text.lines() {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("\"path\"") {
                    let lib = rest.trim().trim_matches('"').replace("\\\\", "/");
                    candidates.push(PathBuf::from(lib).join("steamapps/common/Dishonored"));
                }
            }
        }
    }
    candidates.into_iter().find(|p| looks_right(p))
}

struct Ctx {
    warnings: Vec<String>,
}

impl Ctx {
    fn f(&mut self, what: &str, v: Option<f32>, placeholder: f32) -> f32 {
        v.unwrap_or_else(|| {
            self.warnings.push(format!("{what}: not found, using placeholder {placeholder}"));
            placeholder
        })
    }
    fn b(&mut self, what: &str, v: Option<bool>, placeholder: bool) -> bool {
        v.unwrap_or_else(|| {
            self.warnings.push(format!("{what}: not found, using placeholder {placeholder}"));
            placeholder
        })
    }
}

fn open(install: &Path, name: &str) -> Result<Package, Error> {
    let p = install.join("DishonoredGame/CookedPCConsole").join(name);
    Package::open(&p).map_err(|e| Error::Package(p, e))
}

fn props_at(pkg: &Package, path: &str) -> Result<Vec<Property>, Error> {
    let i = pkg.find_export(path).ok_or_else(|| Error::Missing(path.to_string()))?;
    pkg.properties(i).map_err(|e| Error::Missing(format!("{path}: {e}")))
}

/// Follows an object-reference property to the referenced export's properties.
fn deref(pkg: &Package, props: &[Property], field: &str) -> Option<Vec<Property>> {
    match lookup(props, field)? {
        Value::Object(i) => match ObjRef::from_index(*i) {
            ObjRef::Export(e) => pkg.properties(e).ok(),
            _ => None,
        },
        _ => None,
    }
}

fn fval(props: &[Property], path: &str) -> Option<f32> {
    lookup(props, path).and_then(Value::as_f32)
}

fn bval(props: &[Property], path: &str) -> Option<bool> {
    lookup(props, path).and_then(Value::as_bool)
}

/// Merges `over` onto `base` by property name and array index.
fn merged(base: &[Property], over: &[Property]) -> Vec<Property> {
    let mut out = base.to_vec();
    for p in over {
        match out.iter_mut().find(|q| q.name == p.name && q.array_index == p.array_index) {
            Some(q) => *q = p.clone(),
            None => out.push(p.clone()),
        }
    }
    out
}

pub fn load(install: &Path, difficulty: Difficulty) -> Result<GameData, Error> {
    if !install.join("DishonoredGame/CookedPCConsole").is_dir() {
        return Err(Error::NotFound(install.to_path_buf()));
    }
    let mut cx = Ctx { warnings: Vec::new() };

    let mut cfg = Ini::default();
    for f in ["DefaultGame.ini", "DefaultPlayer.ini", "DefaultPlayerState.ini", "DefaultCamera.ini", "DefaultPower.ini"] {
        match Ini::load(&install.join("DishonoredGame/Config").join(f)) {
            Ok(i) => cfg.merge(i),
            Err(e) => cx.warnings.push(format!("{f}: {e}")),
        }
    }

    let startup = open(install, "Startup.upk")?;
    let engine = open(install, "Engine.upk")?;
    let engine_pawn = props_at(&engine, "Default__Pawn").unwrap_or_default();
    let phys_vol = props_at(&engine, "Default__PhysicsVolume").unwrap_or_default();
    let water_vol = props_at(&engine, "Default__WaterVolume").unwrap_or_default();

    // --- player tweak tree (Startup.upk) ---
    let top = props_at(&startup, "Twk_Pawn_Corvo.Twk_Pawn_Corvo_Release")?;
    let attr_field = format!("m_pAttributeTweaks[{}]", difficulty.index());
    let attrs = deref(&startup, &top, &attr_field).ok_or_else(|| Error::Missing(attr_field.clone()))?;
    let game = open(install, "DishonoredGame.upk")?;
    // Attributes Corvo's tweak leaves out keep the class defaults.
    let attrs = merged(&props_at(&game, "Default__DisTweaks_PlayerPawn_Attributes").unwrap_or_default(), &attrs);
    let mantle = deref(&startup, &top, "m_pMantleTweaks").unwrap_or_default();
    let mantle_blink = deref(&startup, &top, "m_pMantleBlinkTweaks").unwrap_or_default();
    let camera = deref(&startup, &top, "m_pCameraTweaks").unwrap_or_default();
    let d = difficulty.field();
    let a = |name: &str| fval(&attrs, &format!("{name}.{d}"));

    // --- pawn defaults (DishonoredGame.upk) ---
    let pawn = props_at(&game, "Default__DishonoredPawn")?;
    let ppawn = props_at(&game, "Default__DishonoredPlayerPawn")?;
    let pawn_cyl = props_at(&game, "Default__DishonoredPawn.CollisionCylinder").unwrap_or_default();
    let ppawn_cyl = props_at(&game, "Default__DishonoredPlayerPawn.CollisionCylinder").unwrap_or_default();
    let pf = |name: &str| fval(&ppawn, name).or_else(|| fval(&pawn, name));

    let mantle_t = |cx: &mut Ctx, m: &[Property], fallback: &[Property], tag: &str| {
        let g = |k: &str| fval(m, k).or_else(|| fval(fallback, k));
        let gb = |k: &str| bval(m, k).or_else(|| bval(fallback, k));
        MantleTuning {
            line_check_step: cx.f(&format!("{tag}.m_LineCheckStepSize"), g("m_LineCheckStepSize"), 10.0),
            min_edge_height: cx.f(&format!("{tag}.m_MantleMinEdgeHeight"), g("m_MantleMinEdgeHeight"), 50.0),
            max_edge_height: cx.f(&format!("{tag}.m_MantleMaxEdgeHeight"), g("m_MantleMaxEdgeHeight"), 200.0),
            low_max_edge_height: cx.f(&format!("{tag}.m_LowMantleMaxEdgeHeight"), g("m_LowMantleMaxEdgeHeight"), 100.0),
            medium_max_edge_height: cx.f(&format!("{tag}.m_MediumMantleMaxEdgeHeight"), g("m_MediumMantleMaxEdgeHeight"), 150.0),
            low_uses_step_up: cx.b(&format!("{tag}.m_bLowMantleUseStepUp"), gb("m_bLowMantleUseStepUp"), false),
            low_step_up_blend_time: cx.f(&format!("{tag}.m_fLowMantleStepUpBlendTime"), g("m_fLowMantleStepUpBlendTime"), 0.2),
            max_vertical_angle_edge_face: cx.f(&format!("{tag}.m_MaxVerticalAngleForEdgeFace"), g("m_MaxVerticalAngleForEdgeFace"), 20.0),
            max_horizontal_angle_edge_face: cx.f(&format!("{tag}.m_MaxHorizontalAngleForEdgeFace"), g("m_MaxHorizontalAngleForEdgeFace"), 45.0),
            max_slope_angle_edge_top: cx.f(&format!("{tag}.m_MaxSlopeAngleForEdgeTop"), g("m_MaxSlopeAngleForEdgeTop"), 45.0),
            edge_search_dist: cx.f(&format!("{tag}.m_MantleEdgeSearchDist"), g("m_MantleEdgeSearchDist"), 50.0),
            forward_move_amount: cx.f(&format!("{tag}.m_fMantleForwardMoveAmount"), g("m_fMantleForwardMoveAmount"), 20.0),
            fall_speed_for_ledge_grab: cx.f(&format!("{tag}.m_fFallSpeedForLedgeGrab"), g("m_fFallSpeedForLedgeGrab"), 1000.0),
            max_fall_speed_for_mantle: cx.f(&format!("{tag}.m_fMaxFallSpeedForMantle"), g("m_fMaxFallSpeedForMantle"), 2000.0),
            ledge_grab_min_edge_height: cx.f(&format!("{tag}.m_fLedgeGrabMantleMinEdgeHeight"), g("m_fLedgeGrabMantleMinEdgeHeight"), 150.0),
        }
    };
    let mantle_tuning = mantle_t(&mut cx, &mantle, &[], "mantle");
    let mantle_blink_tuning = mantle_t(&mut cx, &mantle_blink, &mantle, "mantle_blink");

    let ini = |s: &str, k: &str| cfg.f32(s, k);
    const LEAN: &str = "DishonoredGame.DishonoredCamera_Lean";
    let lean_consts = cfg.get(LEAN, "m_LeanConstants").map(parse_spring);

    let player = PlayerTuning {
        gravity_z: cx.f("WorldInfo.DefaultGravityZ", ini("Engine.WorldInfo", "DefaultGravityZ"), -1000.0),
        collision_radius: cx.f("player CollisionRadius", fval(&ppawn_cyl, "CollisionRadius").or(fval(&pawn_cyl, "CollisionRadius")), 30.0),
        collision_half_height: cx.f("pawn CollisionHeight", fval(&ppawn_cyl, "CollisionHeight").or(fval(&pawn_cyl, "CollisionHeight")), 80.0),
        crouch_radius: cx.f("CrouchRadius", pf("CrouchRadius"), 30.0),
        crouch_half_height: cx.f("CrouchHeight", pf("CrouchHeight"), 40.0),
        max_step_height: cx.f("MaxStepHeight", pf("MaxStepHeight"), 30.0),
        base_eye_height: cx.f("BaseEyeHeight", pf("BaseEyeHeight"), 60.0),
        max_fall_speed: cx.f("m_fMaxFallSpeed", ini("DishonoredGame.DishonoredPlayerPawn", "m_fMaxFallSpeed").or(pf("MaxFallSpeed")), 2000.0),
        ladder_speed: cx.f("LadderSpeed", pf("LadderSpeed"), 200.0),
        walkable_floor_z: cx.f("WalkableFloorZ", pf("WalkableFloorZ").or(fval(&engine_pawn, "WalkableFloorZ")), 0.7),
        ground_friction: cx.f("GroundFriction", fval(&phys_vol, "GroundFriction"), 8.0),
        water_friction: cx.f("WaterVolume FluidFriction", fval(&water_vol, "FluidFriction"), 2.0),

        ground_speed: cx.f("m_GroundSpeed", a("m_GroundSpeed"), 400.0),
        ground_speed_sprint: cx.f("m_GroundSpeedSprint", a("m_GroundSpeedSprint"), 600.0),
        ground_speed_crouch: cx.f("m_GroundSpeedCrouch", a("m_GroundSpeedCrouch"), 250.0),
        ground_speed_walk: cx.f("m_GroundSpeedWalk", a("m_GroundSpeedWalk"), 200.0),
        ground_speed_slow_walk: cx.f("m_GroundSpeedSlowWalk", a("m_GroundSpeedSlowWalk"), 100.0),
        water_speed: cx.f("m_WaterSpeed", a("m_WaterSpeed"), 400.0),
        accel_rate: cx.f("m_AccelerationRate", a("m_AccelerationRate"), 2000.0),
        strafe_mult_run: cx.f("m_GroundStrafeMultiplierRun", a("m_GroundStrafeMultiplierRun"), 1.0),
        strafe_mult_sneak: cx.f("m_GroundStrafeMultiplierSneak", a("m_GroundStrafeMultiplierSneak"), 1.0),
        strafe_mult_sprint: cx.f("m_GroundStrafeMultiplierSprint", a("m_GroundStrafeMultiplierSprint"), 1.0),
        backward_mult_run: cx.f("m_GroundBackwardMultiplierRun", a("m_GroundBackwardMultiplierRun"), 1.0),
        backward_mult_sneak: cx.f("m_GroundBackwardMultiplierSneak", a("m_GroundBackwardMultiplierSneak"), 1.0),
        backward_mult_sprint: cx.f("m_GroundBackwardMultiplierSprint", a("m_GroundBackwardMultiplierSprint"), 1.0),
        jump_z_attribute: cx.f("m_JumpZ", a("m_JumpZ"), 500.0),
        jump_style: match cfg.get("DishonoredGame.StatePlayerMasterJump", "m_JumpStyle").and_then(|v| v.strip_prefix("eDisJumpStyle_FixedNormalJump_")) {
            Some("HeldPowerJump_FullStop") => JumpStyle::HeldPowerFullStop,
            Some("ContinuousPowerJump") => JumpStyle::ContinuousPower,
            _ => JumpStyle::FixedPower,
        },
        // Without Agility these are all zero, and UE3 leaves zero values out.
        jump_z_power_jump: a("m_JumpZ_PowerJump").unwrap_or(0.0),
        power_jump_full_stop: a("m_PowerJumpFullStop_PROTOTYPE").unwrap_or(0.0),
        full_stop_extra_stop_vel: a("m_FullStop_ExtraStopVel_PROTOTYPE").unwrap_or(0.0),
        held_power_jump_button_time: a("m_HeldPowerJumpButtonTime_PROTOTYPE").unwrap_or(0.0),
        held_power_jump_accel: a("m_HeldPowerJumpAccel_PROTOTYPE").unwrap_or(0.0),
        jump_impulse: cx.f("m_fJumpImpulse", fval(&top, "m_fJumpImpulse"), 500.0),
        air_control: cx.f("m_fAirControl", fval(&top, "m_fAirControl"), 0.1),
        max_speed_before_fall_damage: cx.f("m_MaxSpeedBeforeFallingDamage", a("m_MaxSpeedBeforeFallingDamage"), 2000.0),
        max_speed_before_fall_death: cx.f("m_MaxSpeedBeforeFallingDeath", a("m_MaxSpeedBeforeFallingDeath"), 3000.0),
        land_anim_rate: cx.f("m_LandAnimRate", a("m_LandAnimRate"), 1.0),
        mantle_anim_rate: cx.f("m_MantleAnimRate", a("m_MantleAnimRate"), 1.0),

        slide_time: cx.f("m_fSlideTime", fval(&top, "m_fSlideTime"), 1.0),
        slide_percent_not_cancelable: cx.f("m_fSlidePercentNotCancelable", fval(&top, "m_fSlidePercentNotCancelable"), 0.5),
        slide_allow_return_to_sprint: cx.b("m_bSlideAllowReturnToSprint", bval(&top, "m_bSlideAllowReturnToSprint"), false),
        auto_crouch_test_distance: cx.f("m_fAutoCrouchTestDistance", fval(&top, "m_fAutoCrouchTestDistance"), 100.0),
        auto_crouch_min_crawl_depth: cx.f("m_fAutoCrouchMinCrawlDepth", fval(&top, "m_fAutoCrouchMinCrawlDepth"), 10.0),

        bob_amount: cx.f("m_BobAmount", ini("DishonoredGame.DishonoredPlayerCamera", "m_BobAmount"), 0.5),
        roll_amount: cx.f("m_RollAmount", ini("DishonoredGame.DishonoredPlayerCamera", "m_RollAmount"), 0.5),
        min_view_pitch: cx.f("m_fMinViewPitch", fval(&camera, "m_fMinViewPitch"), -85.0),
        max_view_pitch: cx.f("m_fMaxViewPitch", fval(&camera, "m_fMaxViewPitch"), 85.0),
        default_fov: cx.f("m_fDefaultFOV", ini("DishonoredGame.DishonoredPlayerCamera", "m_fDefaultFOV"), 75.0),
        fov_blend_speed: cx.f("m_fDefaultFOVBlendSpeed", ini("DishonoredGame.DishonoredPlayerCamera", "m_fDefaultFOVBlendSpeed"), 5.0),

        lean: LeanTuning {
            lean_speed: cx.f("lean speed", ini("DishonoredGame.StatePlayerMasterLeaning", "m_fLeanSpeed"), 500.0),
            release_time: cx.f("lean release", ini("DishonoredGame.StatePlayerMasterLeaning", "m_fLeanReleaseTime"), 0.2),
            max_angle: cx.f("lean max angle", ini(LEAN, "m_fMaxLeanAngle"), 15.0),
            max_angle_crouched: cx.f("lean max angle crouched", ini(LEAN, "m_fMaxLeanAngle_Crouched"), 15.0),
            camera_tilt_percent: cx.f("lean tilt", ini(LEAN, "m_fCameraTiltPercent"), 0.5),
            height_pct: cx.f("lean height pct", ini(LEAN, "m_fLeanHeightPct"), 1.0),
            springiness: cx.f("lean springiness", lean_consts.map(|c| c.0), 80.0),
            damping: cx.f("lean damping", lean_consts.map(|c| c.1), 12.0),
        },
        swim: SwimTuning {
            min_accel: cx.f("swim min accel", ini("DishonoredGame.StatePlayerMasterSwim", "m_fMinSwimAccel"), 500.0),
            max_accel: cx.f("swim max accel", ini("DishonoredGame.StatePlayerMasterSwim", "m_fMaxSwimAccel"), 2000.0),
            max_speed_no_stroke: cx.f("swim max no-stroke", ini("DishonoredGame.StatePlayerMasterSwim", "m_fMaxSpeedNoStroke"), 300.0),
            stroke_time: cx.f("swim stroke", ini("DishonoredGame.StatePlayerMasterSwim", "m_fStrokeTime"), 0.3),
            surface_stroke_delay: cx.f("swim surface delay", ini("DishonoredGame.StatePlayerMasterSwim", "m_fSurfaceSwimDelayBetweenStrokes"), 1.0),
        },
        mantle: mantle_tuning,
        mantle_blink: mantle_blink_tuning,
        fall_stun_velocity: cx.f("m_fFall_Stun_Vel", ini("DishonoredGame.StatePlayerMasterFalling", "m_fFall_Stun_Vel"), 2000.0),
    };

    // --- Blink (DishonoredGame.upk + DefaultPower.ini) ---
    let twk = props_at(&game, "Twk_Powers.Blink.Twk_Blink")?;
    let level_defaults = game
        .find_export("DisTweaks_Blink.PowerAttributes_Blink")
        .and_then(|i| game.struct_defaults(i).ok())
        .unwrap_or_default();
    let mut levels = Vec::new();
    for n in 0..3 {
        let Some(Value::Struct { fields, .. }) = lookup(&twk, &format!("m_Levels[{n}]")) else { continue };
        let l = merged(&level_defaults, fields);
        let g = |k: &str| fval(&l, k);
        let tag = |k: &str| format!("blink level {n} {k}");
        levels.push(BlinkLevel {
            distance: cx.f(&tag("m_Distance"), g("m_Distance"), 1000.0),
            horiz_distance: cx.f(&tag("m_HorizDistance"), g("m_HorizDistance"), 1000.0),
            vert_distance: cx.f(&tag("m_VertDistance"), g("m_VertDistance"), 400.0),
            step_distance: cx.f(&tag("m_fDefaultBlinkStepDistance"), g("m_fDefaultBlinkStepDistance"), 100.0),
            step_interval: cx.f(&tag("m_fDefaultTimeBetweenBlinkSteps"), g("m_fDefaultTimeBetweenBlinkSteps"), 0.01),
            warmup_time: cx.f(&tag("m_fWarmupTime"), g("m_fWarmupTime"), 0.5),
            cooldown_time: cx.f(&tag("m_fCooldownTime"), g("m_fCooldownTime"), 0.5),
            warmup_wobble_max: g("m_fWarmupWobbleMaxStrength").unwrap_or(0.0),
            warmup_wobble_per_second: g("m_fWarmupWobblePerSecond").unwrap_or(0.0),
            warmup_distortion_min: g("m_fWarmupDistorionMinStrength").unwrap_or(0.0),
            move_distortion_max: g("m_fMoveDistortionMaxStrength").unwrap_or(0.0),
            move_blur_max: g("m_fMoveBlurMaxStrength").unwrap_or(0.0),
            move_reach_max_at_pct: g("m_fMoveReachMaxAtPercentage").unwrap_or(1.0),
            cooldown_wobble_count: lookup(&l, "m_CoolDownWobbleCount").and_then(Value::as_f32).unwrap_or(0.0) as i32,
        });
    }
    if levels.is_empty() {
        return Err(Error::Missing("Twk_Blink.m_Levels".into()));
    }
    let twk_defaults = game
        .find_export("Default__DisTweaks_Blink")
        .and_then(|i| game.properties(i).ok())
        .unwrap_or_default();
    let tw = |k: &str| fval(&twk, k).or_else(|| fval(&twk_defaults, k));
    let extent = match lookup(&twk, "m_TargetTestExtent") {
        Some(Value::Vector(v)) => *v,
        _ => {
            cx.warnings.push("m_TargetTestExtent not found".into());
            [16.0; 3]
        }
    };
    const BLINK: &str = "DishonoredGame.DishonoredActivePowerComponent_Blink";
    let event_name = |field: &str| match lookup(&twk, &format!("m_Levels[0].{field}")) {
        Some(Value::Object(i)) if *i != 0 => {
            let path = game.object_path(ObjRef::from_index(*i));
            path.rsplit('.').next().map(str::to_string)
        }
        _ => None,
    };
    let sound_events = sounds::BlinkSoundEvents {
        warmup: event_name("m_pBlinkWarmupSoundEvent"),
        blink: event_name("m_pBlinkSoundEvent"),
        fizzle: event_name("m_pFizzleSoundEvent"),
    };
    let blink = BlinkTuning {
        levels,
        sound_events,
        target_test_extent: extent,
        close_collision_distance: cx.f("m_fCloseCollisionDistance", tw("m_fCloseCollisionDistance"), 200.0),
        close_collision_offset_step: cx.f("m_fCloseCollisionOffsetStep", tw("m_fCloseCollisionOffsetStep"), 20.0),
        // UE3 omits properties equal to their (zero) defaults, so absence means 0 here.
        fall_threshold: tw("m_fFallThreshold").unwrap_or(0.0),
        ground_mesh_height: tw("m_fGroundMeshHeight").unwrap_or(0.0),
        limit_vertical_from_ground: bval(&twk, "m_bLimitVerticalDistanceFromGround")
            .or_else(|| bval(&twk_defaults, "m_bLimitVerticalDistanceFromGround"))
            .unwrap_or(false),
        mana_cost: lookup(&twk, "m_ManaCost").and_then(Value::as_f32).unwrap_or(0.0) as i32,
        impulse_radius: cfg.f32(BLINK, "m_fImpulseRadius").unwrap_or(0.0),
        impulse_strength: cfg.f32(BLINK, "m_fImpulseStrength").unwrap_or(0.0),
    };

    let mut anim_lengths = std::collections::HashMap::new();
    for i in startup.exports_of_class("AnimSequence") {
        if !startup.object_path(ObjRef::Export(i)).starts_with("Ply_") {
            continue;
        }
        let Ok(p) = startup.properties(i) else { continue };
        let (Some(Value::Name(name)), Some(len)) = (lookup(&p, "SequenceName"), fval(&p, "SequenceLength")) else { continue };
        let rate = fval(&p, "RateScale").filter(|r| *r > 0.0).unwrap_or(1.0);
        anim_lengths.entry(name.clone()).or_insert(len / rate);
    }
    if anim_lengths.is_empty() {
        cx.warnings.push("no player animations found in Startup.upk".into());
    }

    let (drop_assassinate, assassinate) = takedown::load(&mut cx, install, &startup, &game);
    let melee = melee::load(&mut cx, &startup, &game, &ppawn, difficulty);

    let powers = powers::parse_powers(cfg.all("DishonoredGame.DishonoredPowersComponent", "m_Powers"));
    if !powers.iter().any(|p| p.name == powers::AGILITY) {
        cx.warnings.push("Agility (Celerity) not found in DefaultPlayer.ini".into());
    }

    Ok(GameData { install: install.to_path_buf(), difficulty, player, blink, drop_assassinate, assassinate, melee, powers, anim_lengths, warnings: cx.warnings })
}

/// Parses `(m_Springiness=80.0,m_Damping=12.0)`.
fn parse_spring(s: &str) -> (f32, f32) {
    let mut spring = 0.0;
    let mut damp = 0.0;
    for part in s.trim_matches(['(', ')']).split(',') {
        if let Some((k, v)) = part.split_once('=') {
            let v: f32 = v.trim().parse().unwrap_or(0.0);
            match k.trim() {
                "m_Springiness" => spring = v,
                "m_Damping" => damp = v,
                _ => {}
            }
        }
    }
    (spring, damp)
}
