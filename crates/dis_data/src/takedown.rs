//! Takedown tuning, read from the install.
//!
//! - The drop assassination: Corvo's `DisTweaks_DropAssassinate`.
//! - The ground assassination: Corvo's `DisTweaks_Assassinate` (with the `DisTweaks_Finisher`
//!   pacing of slow and fast kills it inherits).
//!
//! Where each paired kill puts Corvo relative to the victim comes from the victims' half of the
//! paired animations (`Generic_Assassination_<...>_Slave`), whose `anchor_jnt` marks the killer's
//! place; those live with the City Watch's animations in the mission packages. How long the player
//! is held is when Corvo's animation releases him (its `DisNotify_AnimStateUnlock`).

use crate::melee::Clips;
use crate::{bval, fval, merged, Ctx};
use edge_anim::{Animation, Skeleton};
use glam::Mat4;
use std::path::Path;
use upk::{lookup, ObjRef, Package, Property, Value};

/// Sides in the game's cardinal-direction order (front, left, right, back).
pub const SIDES: [&str; 4] = ["Front", "Left", "Right", "Back"];

/// Mission packages carrying the City Watch's paired animations; the first one found is used.
const VICTIM_PACKAGES: [&str; 3] = ["L_Streets1_P.upk", "L_Distillery_P.upk", "L_Distillery_Ext_Script.upk"];

#[derive(Clone, Debug)]
pub struct DropSide {
    /// The killer's feet in the victim's frame (Unreal units: x forward, y right, z up, from its feet).
    pub anchor: [f32; 3],
    /// How long Corvo is held (seconds): until his animation unlocks him, else its full length.
    pub duration: f32,
    /// Corvo's animation.
    pub anim: String,
}

#[derive(Clone, Debug)]
pub struct DropAssassinateTuning {
    pub hit_window: f32,
    pub min_drop_dist: f32,
    pub max_drop_dist: f32,
    pub max_drop_jump_vel: f32,
    pub min_drop_down_vel: f32,
    /// Indexed like [`SIDES`].
    pub sides: [DropSide; 4],
}

#[derive(Clone, Debug)]
pub struct AssassinateTuning {
    pub range: f32,
    pub ray_scale_percent: f32,
    pub probe_extent: [f32; 3],
    /// `m_AssassinateOnAwareness[EAIAwareness].m_bAssassinate`.
    pub on_awareness: [bool; 8],
    pub can_assassinate_runners: bool,
    pub finishers_before_slow: (u32, u32),
    pub time_before_slow: (f32, f32),
    /// Indexed like [`SIDES`].
    pub slow: [DropSide; 4],
    pub fast: [DropSide; 4],
    /// The unpaired kill (`Sword_Ready_Assassination_Generic`); its anchor is unused.
    pub generic: DropSide,
}

/// Properties of the first export of `class` whose path starts with `prefix`.
fn tweak(pkg: &Package, class: &str, prefix: &str) -> Option<Vec<Property>> {
    let i = pkg.exports_of_class(class).find(|&i| pkg.object_path(ObjRef::Export(i)).starts_with(prefix))?;
    pkg.properties(i).ok()
}

/// Corvo's tweak of `class` over the shared sword tweak over the class defaults (`chain`, base
/// class first).
fn sword_tweak(cx: &mut Ctx, startup: &Package, game: &Package, class: &str, chain: &[&str]) -> Vec<Property> {
    let mut t = Vec::new();
    for c in chain {
        let d = game.find_export(&format!("Default__{c}")).and_then(|i| game.properties(i).ok()).unwrap_or_default();
        t = merged(&t, &d);
    }
    t = merged(&t, &tweak(startup, class, "Twk_Inv_SwordBase.").unwrap_or_default());
    match tweak(startup, class, "Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo.") {
        Some(c) => merged(&t, &c),
        None => {
            cx.warnings.push(format!("Corvo's {class} not found; using the shared sword tweak"));
            t
        }
    }
}

pub(crate) fn load(cx: &mut Ctx, install: &Path, startup: &Package, game: &Package) -> (DropAssassinateTuning, AssassinateTuning) {
    let clips = Clips::new(startup);
    // How long Corvo is held: until the clip's unlock notify, else its whole length.
    let held = |cx: &mut Ctx, clip: String, anchor: [f32; 3]| {
        let unlock = clips.notifies(&clip).iter().find(|(_, c, _)| c == "DisNotify_AnimStateUnlock").map(|(t, _, _)| *t);
        let at_rate = clips.length(&clip).map(|(len, rate)| unlock.unwrap_or(len) / rate);
        DropSide { anchor, duration: cx.f(&clip, at_rate, 2.0), anim: clip }
    };

    // Every victim anchor, from one mission package.
    let mut slaves = Vec::new();
    for side in SIDES {
        slaves.push(format!("Generic_Assassination_Drop{side}_Slave"));
    }
    for pace in ["", "Fast"] {
        for side in SIDES {
            slaves.push(format!("Generic_Assassination_{pace}{side}_Slave"));
        }
    }
    let anchors = victim_anchors(install, &slaves).unwrap_or_else(|e| {
        cx.warnings.push(format!("takedown anchors: {e}; using placeholders"));
        let ring = [[70.0, 0.0, 0.0], [0.0, -70.0, 0.0], [0.0, 70.0, 0.0], [-70.0, 0.0, 0.0]];
        ring.iter().chain(&ring).chain(&ring).copied().collect()
    });

    // --- drop assassination ---
    let t = sword_tweak(cx, startup, game, "DisTweaks_DropAssassinate", &["DisTweaks_ItemContext", "DisTweaks_DropAssassinate"]);
    // UE3 leaves out properties equal to zero, so a missing field reads as 0.
    let z = |k: &str| fval(&t, k).unwrap_or(0.0);
    let sides = std::array::from_fn(|i| held(cx, format!("Sword_Ready_Assassination_Drop{}_Master", SIDES[i]), anchors[i]));
    let drop = DropAssassinateTuning {
        hit_window: cx.f("m_fHitWindowInSeconds", fval(&t, "m_fHitWindowInSeconds"), 1.0),
        min_drop_dist: z("m_fMinDropDistToTarget"),
        max_drop_dist: cx.f("m_fMaxDropDistToTarget", fval(&t, "m_fMaxDropDistToTarget"), 300.0),
        max_drop_jump_vel: z("m_fMaxAllowedDropJumpVel"),
        min_drop_down_vel: z("m_fMinDropDownVel"),
        sides,
    };

    // --- ground assassination ---
    let chain = ["DisTweaks_ItemContext", "DisTweaks_MeleeAttack", "DisTweaks_MeleeAttackPlayer", "DisTweaks_Finisher", "DisTweaks_Assassinate"];
    let t = sword_tweak(cx, startup, game, "DisTweaks_Assassinate", &chain);
    let int = |k: &str| match lookup(&t, k) {
        Some(Value::Int(v)) => Some(*v),
        _ => None,
    };
    let probe = match lookup(&t, "m_Assassination_Generic_ProbeExtents") {
        Some(Value::Vector(v)) => *v,
        _ => {
            cx.warnings.push("m_Assassination_Generic_ProbeExtents not found".into());
            [20.0; 3]
        }
    };
    let slow = std::array::from_fn(|i| held(cx, format!("Sword_Ready_Assassination_{}_Master", SIDES[i]), anchors[4 + i]));
    let fast = std::array::from_fn(|i| held(cx, format!("Sword_Ready_Assassination_Fast{}_Master", SIDES[i]), anchors[8 + i]));
    let generic = held(cx, "Sword_Ready_Assassination_Generic".into(), [0.0; 3]);
    let assassinate = AssassinateTuning {
        range: cx.f("m_fMaxContextRange (assassinate)", fval(&t, "m_fMaxContextRange"), 250.0),
        ray_scale_percent: fval(&t, "m_fRayScalePercent").unwrap_or(0.0),
        probe_extent: probe,
        on_awareness: std::array::from_fn(|i| bval(&t, &format!("m_AssassinateOnAwareness[{i}].m_bAssassinate")).unwrap_or(false)),
        can_assassinate_runners: bval(&t, "m_bCanAssassinateRunners").unwrap_or(false),
        finishers_before_slow: (int("m_NumFinishersBeforeSlow_Min").unwrap_or(0).max(0) as u32, int("m_NumFinishersBeforeSlow_Max").unwrap_or(0).max(0) as u32),
        time_before_slow: (fval(&t, "m_fTimeBeforeSlow_Min").unwrap_or(0.0), fval(&t, "m_fTimeBeforeSlow_Max").unwrap_or(0.0)),
        slow,
        fast,
        generic,
    };
    (drop, assassinate)
}

/// `anchor_jnt` at the start of each named victim animation, in Unreal axes.
fn victim_anchors(install: &Path, names: &[String]) -> Result<Vec<[f32; 3]>, String> {
    let dir = install.join("DishonoredGame/CookedPCConsole");
    let path = VICTIM_PACKAGES.iter().map(|p| dir.join(p)).find(|p| p.is_file()).ok_or("no mission package with the guards' animations")?;
    let pkg = Package::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Any skeleton of the shared human rig (it has the anchor joint) can evaluate the clips.
    let rigs: Vec<(upk::skelmesh::SkeletalMesh, Skeleton)> = pkg
        .exports_of_class("SkeletalMesh")
        .filter_map(|i| pkg.skeletal_mesh(i).ok())
        .filter(|m| m.bones.iter().any(|b| b.name == "anchor_jnt"))
        .filter_map(|m| Skeleton::parse(&m.edge_skeleton).ok().map(|s| (m, s)))
        .collect();
    let mut by_name = std::collections::HashMap::new();
    for i in pkg.exports_of_class("AnimSequence") {
        let Ok((p, tail)) = pkg.properties_and_tail(i) else { continue };
        if let Some(Value::Name(n)) = lookup(&p, "SequenceName") {
            if names.contains(n) {
                by_name.entry(n.clone()).or_insert((i, tail));
            }
        }
    }
    names
        .iter()
        .map(|name| {
            let &(i, tail) = by_name.get(name).ok_or_else(|| format!("{name} not in {}", path.display()))?;
            let d = pkg.export_data(i);
            let anim = d
                .get(tail + 4..tail + 8)
                .and_then(|b| b.try_into().ok())
                .map(|b| u32::from_le_bytes(b) as usize)
                .and_then(|len| d.get(tail + 8..tail + 8 + len))
                .and_then(|blob| Animation::parse(blob).ok())
                .ok_or_else(|| format!("{name}: not an Edge animation"))?;
            let anchor = rigs
                .iter()
                .find_map(|(mesh, skel)| {
                    let pose = anim.evaluate(skel, 0.0, false).ok()?;
                    let mut world: Vec<Mat4> = Vec::with_capacity(pose.len());
                    for (i, j) in pose.iter().enumerate() {
                        let local = Mat4::from_rotation_translation(j.rotation, j.translation);
                        let parent = mesh.bones.get(i)?.parent;
                        world.push(if i == 0 || parent == i { local } else { *world.get(parent)? * local });
                    }
                    let a = mesh.bones.iter().position(|b| b.name == "anchor_jnt")?;
                    Some(world.get(a)?.w_axis.truncate())
                })
                .ok_or_else(|| format!("{name}: no skeleton evaluates it"))?;
            // Mesh space is Y-down with +Z forward and +X to the character's left.
            Ok([anchor.z, -anchor.x, -anchor.y])
        })
        .collect()
}
