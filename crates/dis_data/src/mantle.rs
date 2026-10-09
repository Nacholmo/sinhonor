//! How the mantle animations move the player, read from the install: the root bone of each
//! `Empty_[Crouch]Mantle<Kind>` clip (the first-person rig's `root0_jnt`) sampled over the clip, and
//! when each clip ends (its `DishonoredNotify_AnimStateExit`).

use crate::melee::Clips;
use crate::Ctx;
use edge_anim::{Animation, Skeleton};
use upk::{lookup, ObjRef, Package, Value};

/// The clips in [`MantleClips::clips`] order: low, medium, high, then the crouched ones.
pub const CLIPS: [&str; 6] = ["Empty_MantleLow", "Empty_MantleMedium", "Empty_MantleHigh", "Empty_CrouchMantleLow", "Empty_CrouchMantleMedium", "Empty_CrouchMantleHigh"];

/// Root samples per clip.
const SAMPLES: usize = 24;

#[derive(Clone, Debug, Default)]
pub struct MantleClip {
    pub name: String,
    /// When the clip ends the mantle (seconds, at its play rate).
    pub exit: f32,
    /// The root's path: (seconds, rise, forward), in Unreal units from where it starts. Empty if
    /// the clip couldn't be decoded.
    pub root: Vec<[f32; 3]>,
}

pub(crate) fn load(cx: &mut Ctx, engine: &Package, startup: &Package) -> [MantleClip; 6] {
    let clips = Clips::new(startup);
    let skeleton = engine
        .find_export("Ply_Player.Skm_Player")
        .and_then(|i| engine.skeletal_mesh(i).ok())
        .and_then(|m| Skeleton::parse(&m.edge_skeleton).ok());
    if skeleton.is_none() {
        cx.warnings.push("first-person skeleton not found; mantles move in a straight line".into());
    }
    std::array::from_fn(|i| {
        let name = CLIPS[i];
        let (len, rate) = match clips.length(name) {
            Some(l) => l,
            None => (cx.f(&format!("{name} length"), None, 1.0), 1.0),
        };
        let exit = clips.notifies(name).iter().find(|(_, c, _)| c == "DishonoredNotify_AnimStateExit").map_or(len, |(t, _, _)| *t) / rate;
        let root = skeleton.as_ref().and_then(|s| root_path(startup, s, name, rate)).unwrap_or_default();
        MantleClip { name: name.to_string(), exit, root }
    })
}

/// The root bone's rise and forward travel over the clip.
fn root_path(pkg: &Package, skeleton: &Skeleton, name: &str, rate: f32) -> Option<Vec<[f32; 3]>> {
    let anim = pkg.exports_of_class("AnimSequence").filter(|&i| pkg.object_path(ObjRef::Export(i)).starts_with("Ply_")).find_map(|i| {
        let (p, tail) = pkg.properties_and_tail(i).ok()?;
        if !matches!(lookup(&p, "SequenceName"), Some(Value::Name(n)) if n == name) {
            return None;
        }
        let d = pkg.export_data(i);
        let len = u32::from_le_bytes(d.get(tail + 4..tail + 8)?.try_into().ok()?) as usize;
        Animation::parse(d.get(tail + 8..tail + 8 + len)?).ok()
    })?;
    let at = |t: f32| anim.evaluate(skeleton, t, false).ok().and_then(|p| p.first().map(|j| j.translation));
    let start = at(0.0)?;
    (0..=SAMPLES)
        .map(|k| {
            let t = anim.duration * k as f32 / SAMPLES as f32;
            // Mesh space is Y-down with +Z forward.
            let p = at(t)? - start;
            Some([t / rate, -p.y, p.z])
        })
        .collect()
}
