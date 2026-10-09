//! Sword-attack tuning, read from the install: Corvo's sword tweak (`DisTweaks_MeleeAttackPlayer`
//! over the shared sword tweak and the class defaults), the reach-by-speed settings on the
//! player pawn, the sword's damage, the default character health, and the timing of every swing
//! animation from its notifies.

use crate::{fval, merged, Ctx, Difficulty};
use std::collections::HashMap;
use upk::{lookup, ObjRef, Package, Property, Value};

#[derive(Clone, Debug)]
pub struct SwingAnim {
    pub name: String,
    /// `DishonoredNotify_AttackZone`: when the blade can hit, and for how long (`m_fDamageZoneMaxTime`).
    pub zone: Option<(f32, f32)>,
    /// `DishonoredNotify_ChainInput`.
    pub chain_input: Option<f32>,
    /// `DishonoredNotify_AttackInterruptable`, else the exit.
    pub interruptible: f32,
    /// `DishonoredNotify_AnimStateExit`, else the clip's length.
    pub exit: f32,
}

#[derive(Clone, Debug)]
pub struct SwingSet {
    pub swing: SwingAnim,
    /// The environment-hit recoil (`..._Big`), and the one for a chained swing (`..._BigChain`).
    pub env_hit: Option<SwingAnim>,
    pub env_hit_chain: Option<SwingAnim>,
}

#[derive(Clone, Debug)]
pub struct MeleeTuning {
    pub range: f32,
    pub ray_scale_percent: f32,
    pub ray_speed_scale: f32,
    pub ray_speed_scale_max: f32,
    pub min_speed_ray_scale: f32,
    pub sweep_size: f32,
    pub crosshair_size: f32,
    pub chain_time: f32,
    pub damage: f32,
    pub env_hit_shake: f32,
    /// Forehand, backhand, sneak, killing forehand, killing backhand; each with its variants.
    pub swings: [Vec<SwingSet>; 5],
    /// Health of a character that has no tweak of its own (`Twk_Pawn_DefaultNPC`).
    pub default_npc_health: f32,
}

/// First-person animations by sequence name: (length, play rate, notifies as (time, class, properties)).
pub(crate) struct Clips<'a> {
    pkg: &'a Package,
    by_name: HashMap<String, Vec<Property>>,
}

impl<'a> Clips<'a> {
    pub fn new(pkg: &'a Package) -> Self {
        let mut by_name = HashMap::new();
        for i in pkg.exports_of_class("AnimSequence") {
            if !pkg.object_path(ObjRef::Export(i)).starts_with("Ply_") {
                continue;
            }
            let Ok(p) = pkg.properties(i) else { continue };
            if let Some(Value::Name(n)) = lookup(&p, "SequenceName") {
                by_name.entry(n.clone()).or_insert(p);
            }
        }
        Clips { pkg, by_name }
    }

    /// Length and play rate.
    pub fn length(&self, name: &str) -> Option<(f32, f32)> {
        let p = self.by_name.get(name)?;
        let rate = fval(p, "RateScale").filter(|r| *r > 0.0).unwrap_or(1.0);
        Some((fval(p, "SequenceLength")?, rate))
    }

    /// The sequence's notifies: (time, notify class, notify properties), in clip time.
    pub fn notifies(&self, name: &str) -> Vec<(f32, String, Vec<Property>)> {
        let Some(p) = self.by_name.get(name) else { return Vec::new() };
        let Some(Value::Array { count, raw }) = lookup(p, "Notifies") else { return Vec::new() };
        let Ok(events) = self.pkg.struct_array(*count, raw) else { return Vec::new() };
        events
            .iter()
            .filter_map(|ev| {
                let Some(Value::Object(n)) = lookup(ev, "Notify") else { return None };
                let ObjRef::Export(ni) = ObjRef::from_index(*n) else { return None };
                let time = lookup(ev, "Time").and_then(Value::as_f32).unwrap_or(0.0);
                Some((time, self.pkg.export_class_name(ni), self.pkg.properties(ni).unwrap_or_default()))
            })
            .collect()
    }

    /// A swing's timing, at its play rate.
    fn swing(&self, name: &str) -> Option<SwingAnim> {
        let (len, rate) = self.length(name)?;
        let notes = self.notifies(name);
        let at = |class: &str| notes.iter().find(|(_, c, _)| c == class).map(|(t, _, _)| t / rate);
        let zone = notes
            .iter()
            .find(|(_, c, _)| c == "DishonoredNotify_AttackZone")
            .map(|(t, _, p)| (t / rate, fval(p, "m_fDamageZoneMaxTime").unwrap_or(0.0) / rate));
        let exit = at("DishonoredNotify_AnimStateExit").unwrap_or(len / rate);
        Some(SwingAnim { name: name.to_string(), zone, chain_input: at("DishonoredNotify_ChainInput"), interruptible: at("DishonoredNotify_AttackInterruptable").unwrap_or(exit), exit })
    }
}

/// Properties of the first export of `class` whose path starts with `prefix`.
fn tweak(pkg: &Package, class: &str, prefix: &str) -> Option<Vec<Property>> {
    let i = pkg.exports_of_class(class).find(|&i| pkg.object_path(ObjRef::Export(i)).starts_with(prefix))?;
    pkg.properties(i).ok()
}

pub(crate) fn load(cx: &mut Ctx, startup: &Package, game: &Package, player_pawn: &[Property], difficulty: Difficulty) -> MeleeTuning {
    const CLASS: &str = "DisTweaks_MeleeAttackPlayer";
    let defaults = |name: &str| game.find_export(name).and_then(|i| game.properties(i).ok()).unwrap_or_default();
    // Class defaults from the base class up, the shared sword tweak, then Corvo's.
    let mut t = Vec::new();
    for layer in [defaults("Default__DisTweaks_ItemContext"), defaults("Default__DisTweaks_MeleeAttack"), defaults("Default__DisTweaks_MeleeAttackPlayer")] {
        t = merged(&t, &layer);
    }
    t = merged(&t, &tweak(startup, CLASS, "Twk_Inv_SwordBase.").unwrap_or_default());
    match tweak(startup, CLASS, "Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo.") {
        Some(c) => t = merged(&t, &c),
        None => cx.warnings.push("Corvo's DisTweaks_MeleeAttackPlayer not found; using the shared sword tweak".into()),
    }
    let d = difficulty.field();
    let attrs = tweak(startup, "DisTweaks_InventoryItem_Attributes", "Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo.");
    let sword_damage = attrs.as_deref().and_then(|a| fval(a, &format!("m_MeleeDamage.{d}")));
    let npc_health = startup
        .find_export("Twk_Pawn_DefaultNPC.Twk_Pawn_DefaultNPC")
        .and_then(|i| startup.properties(i).ok())
        .and_then(|p| match lookup(&p, &format!("m_pAttributeTweaks[{}]", difficulty as usize)) {
            Some(Value::Object(o)) => match ObjRef::from_index(*o) {
                ObjRef::Export(e) => startup.properties(e).ok(),
                _ => None,
            },
            _ => None,
        })
        .and_then(|a| fval(&a, &format!("m_HealthMax.{d}")));

    let clips = Clips::new(startup);
    let mut missing = Vec::new();
    let mut get = |name: &str| {
        let s = clips.swing(name);
        if s.is_none() {
            missing.push(name.to_string());
        }
        s
    };
    // Swing animations by the game's animation states: forehand = `RightAttackA/B`, backhand =
    // `LeftAttackA/B`, sneak = `SneakAttackA`, killing blows = `Fatality_GenericB` (forehand)
    // and `Fatality_GenericA` (backhand).
    let mut swings: [Vec<SwingSet>; 5] = Default::default();
    for (i, side) in ["Right", "Left"].into_iter().enumerate() {
        for v in ["A", "B"] {
            let base = format!("Sword_Ready_Attack{side}_{v}");
            if let Some(swing) = get(&format!("{base}_Small")) {
                swings[i].push(SwingSet { swing, env_hit: get(&format!("{base}_Big")), env_hit_chain: get(&format!("{base}_BigChain")) });
            }
        }
    }
    if let Some(swing) = get("Sword_Sneak_Attack_Small") {
        swings[2].push(SwingSet { swing, env_hit: get("Sword_Sneak_Attack_BigWall"), env_hit_chain: None });
    }
    for (i, name) in [(3, "Sword_Ready_Fatality_Generic_Right_A"), (4, "Sword_Ready_Fatality_Generic_Left_A")] {
        if let Some(swing) = get(name) {
            swings[i].push(SwingSet { swing, env_hit: None, env_hit_chain: None });
        }
    }
    for m in missing {
        cx.warnings.push(format!("sword animation {m} not found"));
    }

    let multiplier = fval(&t, "m_fAttackDamageMultiplier").unwrap_or(1.0);
    MeleeTuning {
        range: cx.f("m_fMaxContextRange (sword)", fval(&t, "m_fMaxContextRange"), 200.0),
        ray_scale_percent: fval(&t, "m_fRayScalePercent").unwrap_or(0.0),
        ray_speed_scale: fval(player_pawn, "m_fRaySpeedScale").unwrap_or(0.0),
        ray_speed_scale_max: fval(player_pawn, "m_fRaySpeedScale_Max").unwrap_or(0.0),
        min_speed_ray_scale: fval(player_pawn, "m_fMinSpeedRayScale").unwrap_or(0.0),
        sweep_size: cx.f("m_fSweepingAttackSize", fval(&t, "m_fSweepingAttackSize"), 50.0),
        crosshair_size: cx.f("m_fCrosshairAttackSize", fval(&t, "m_fCrosshairAttackSize"), 15.0),
        chain_time: cx.f("m_fMaxChainAttackTime", fval(&t, "m_fMaxChainAttackTime"), 1.0),
        damage: cx.f("sword m_MeleeDamage", sword_damage, 10.0) * multiplier,
        env_hit_shake: fval(&t, "m_fCamShake_OnHitEnv").unwrap_or(0.0),
        swings,
        default_npc_health: cx.f("Twk_Pawn_DefaultNPC m_HealthMax", npc_health, 25.0),
    }
}
