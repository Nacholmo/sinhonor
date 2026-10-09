//! Drop-assassination tuning, read from the install: Corvo's sword tweak
//! (`DisTweaks_DropAssassinate`), the lengths of his four kill animations, and where each side's
//! kill puts him relative to the victim. That last part comes from the victims' half of the
//! paired animations (`Generic_Assassination_Drop<Side>_Slave`), whose `anchor_jnt` marks the
//! killer's place; those live with the City Watch's animations in the mission packages. How long
//! the player is held is when Corvo's animation releases him (its `DisNotify_AnimStateUnlock`).

use crate::{fval, merged, Ctx};
use edge_anim::{Animation, Skeleton};
use glam::Mat4;
use std::path::Path;
use upk::{lookup, ObjRef, Package, Property, Value};

/// Sides in the game's cardinal-direction order (front, left, right, back).
pub const SIDES: [&str; 4] = ["Front", "Left", "Right", "Back"];

/// Mission packages carrying the City Watch's paired animations; the first one found is used.
const VICTIM_PACKAGES: [&str; 3] = ["L_Streets1_P.upk", "L_Distillery_P.upk", "L_Distillery_Ext_Script.upk"];

#[derive(Clone, Copy, Debug)]
pub struct DropSide {
    /// The killer's feet in the victim's frame (Unreal units: x forward, y right, z up, from its feet).
    pub anchor: [f32; 3],
    /// How long Corvo is held for this side's kill (seconds): until his animation unlocks him,
    /// else its full length.
    pub duration: f32,
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

/// Properties of the first export of `class` whose path starts with `prefix`.
fn tweak(pkg: &Package, class: &str, prefix: &str) -> Option<Vec<Property>> {
    let i = pkg.exports_of_class(class).find(|&i| pkg.object_path(ObjRef::Export(i)).starts_with(prefix))?;
    pkg.properties(i).ok()
}

pub(crate) fn load(
    cx: &mut Ctx,
    install: &Path,
    startup: &Package,
    game: &Package,
    anim_lengths: &std::collections::HashMap<String, f32>,
) -> DropAssassinateTuning {
    // Corvo's sword, over the shared sword tweak it falls back to, over the class defaults.
    const CLASS: &str = "DisTweaks_DropAssassinate";
    let defaults = game.find_export("Default__DisTweaks_DropAssassinate").and_then(|i| game.properties(i).ok()).unwrap_or_default();
    let base = tweak(startup, CLASS, "Twk_Inv_SwordBase.").unwrap_or_default();
    let corvo = tweak(startup, CLASS, "Twk_Inv_PlayerSpecific.Twk_Inv_SwordCorvo.");
    if corvo.is_none() {
        cx.warnings.push("Corvo's DisTweaks_DropAssassinate not found; using the shared sword tweak".into());
    }
    let t = merged(&merged(&defaults, &base), &corvo.unwrap_or_default());
    // UE3 leaves out properties equal to zero, so a missing field reads as 0.
    let z = |k: &str| fval(&t, k).unwrap_or(0.0);

    let anchors = victim_anchors(install).unwrap_or_else(|e| {
        cx.warnings.push(format!("drop assassination anchors: {e}; using placeholders"));
        [[70.0, 0.0, 0.0], [0.0, -70.0, 0.0], [0.0, 70.0, 0.0], [-70.0, 0.0, 0.0]]
    });
    let sides = std::array::from_fn(|i| {
        let clip = format!("Sword_Ready_Assassination_Drop{}_Master", SIDES[i]);
        let held = unlock_time(startup, &clip).or_else(|| anim_lengths.get(&clip).copied());
        DropSide { anchor: anchors[i], duration: cx.f(&clip, held, 2.0) }
    });
    DropAssassinateTuning {
        hit_window: cx.f("m_fHitWindowInSeconds", fval(&t, "m_fHitWindowInSeconds"), 1.0),
        min_drop_dist: z("m_fMinDropDistToTarget"),
        max_drop_dist: cx.f("m_fMaxDropDistToTarget", fval(&t, "m_fMaxDropDistToTarget"), 300.0),
        max_drop_jump_vel: z("m_fMaxAllowedDropJumpVel"),
        min_drop_down_vel: z("m_fMinDropDownVel"),
        sides,
    }
}

/// When a player animation hands control back (its `DisNotify_AnimStateUnlock`).
fn unlock_time(startup: &Package, clip: &str) -> Option<f32> {
    let p = startup.exports_of_class("AnimSequence").find_map(|i| {
        let p = startup.properties(i).ok()?;
        matches!(lookup(&p, "SequenceName"), Some(Value::Name(n)) if n == clip).then_some(p)
    })?;
    let Some(Value::Array { count, raw }) = lookup(&p, "Notifies") else { return None };
    startup.struct_array(*count, raw).ok()?.iter().find_map(|ev| {
        let Some(Value::Object(n)) = lookup(ev, "Notify") else { return None };
        let ObjRef::Export(ni) = ObjRef::from_index(*n) else { return None };
        (startup.export_class_name(ni) == "DisNotify_AnimStateUnlock").then(|| lookup(ev, "Time").and_then(Value::as_f32))?
    })
}

/// `anchor_jnt` at the start of each side's victim animation, in Unreal axes.
fn victim_anchors(install: &Path) -> Result<[[f32; 3]; 4], String> {
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
    let mut out = [[0.0; 3]; 4];
    for (side, slot) in SIDES.iter().zip(out.iter_mut()) {
        let name = format!("Generic_Assassination_Drop{side}_Slave");
        let anim = pkg
            .exports_of_class("AnimSequence")
            .find_map(|i| {
                let (p, tail) = pkg.properties_and_tail(i).ok()?;
                matches!(lookup(&p, "SequenceName"), Some(Value::Name(n)) if *n == name).then_some(())?;
                let d = pkg.export_data(i);
                let len = u32::from_le_bytes(d.get(tail + 4..tail + 8)?.try_into().ok()?) as usize;
                Animation::parse(d.get(tail + 8..tail + 8 + len)?).ok()
            })
            .ok_or_else(|| format!("{name} not in {}", path.display()))?;
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
        *slot = [anchor.z, -anchor.x, -anchor.y];
    }
    Ok(out)
}
