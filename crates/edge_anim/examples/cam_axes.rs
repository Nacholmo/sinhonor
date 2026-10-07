//! Dev check: pose the arms (base + left overlay) and express the hands in camera_jnt space.
use edge_anim::{Animation, Joint, Skeleton};
use glam::{Mat4, Quat, Vec3};
use upk::{lookup, Package, Value};

fn anim(pkg: &Package, name: &str) -> Animation {
    for i in pkg.exports_of_class("AnimSequence") {
        let Ok((p, tail)) = pkg.properties_and_tail(i) else { continue };
        if !matches!(lookup(&p, "SequenceName"), Some(Value::Name(n)) if n == name) { continue }
        let d = pkg.export_data(i);
        let len = u32::from_le_bytes(d[tail + 4..tail + 8].try_into().unwrap()) as usize;
        return Animation::parse(&d[tail + 8..tail + 8 + len]).unwrap();
    }
    panic!("{name} not found")
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let engine = Package::open(dir.join("Engine.upk")).unwrap();
    let startup = Package::open(dir.join("Startup.upk")).unwrap();
    let mesh = engine.skeletal_mesh(engine.find_export("Ply_Player.Skm_Player").unwrap()).unwrap();
    let skel = Skeleton::parse(&mesh.edge_skeleton).unwrap();
    let base = anim(&startup, "Sword_Ready_Idle");
    let over = anim(&startup, "Powers_Idle");
    let mut pose = base.evaluate(&skel, 0.0, false).unwrap();
    let op = over.evaluate(&skel, 0.0, false).unwrap();
    for h in &over.joint_hashes { if let Some(j) = skel.joint_by_hash(*h) { if mesh.bones[j].name.contains("_L_") || mesh.bones[j].name.contains("_L") { pose[j] = op[j]; } } }
    let mut world: Vec<Mat4> = Vec::new();
    for (i, Joint { rotation, translation }) in pose.iter().enumerate() {
        let local = Mat4::from_rotation_translation(*rotation, *translation);
        let p = mesh.bones[i].parent;
        world.push(if i == 0 || p == i { local } else { world[p] * local });
    }
    let cam = mesh.bones.iter().position(|b| b.name == "camera_jnt").unwrap();
    let inv = world[cam].inverse();
    for name in ["hand_L_jnt", "hand_R_jnt", "handAttachment_R_jnt", "head_end_jnt", "spine_3_jnt"] {
        let j = mesh.bones.iter().position(|b| b.name == name).unwrap();
        println!("{name:22} in camera_jnt space: {:?}", inv.transform_point3(world[j].w_axis.truncate()));
    }
    // Sword tip (sword space +Z 65) under candidate socket rotations, in camera_jnt space
    // (+X up, +Y right, -Z forward).
    let attach = mesh.bones.iter().position(|b| b.name == "handAttachment_R_jnt").unwrap();
    let base = inv * world[attach];
    for (label, r) in [("none", Mat4::IDENTITY), ("Rz+90", Mat4::from_rotation_z(1.5708)), ("Rz-90", Mat4::from_rotation_z(-1.5708)), ("Ry+90", Mat4::from_rotation_y(1.5708)), ("Ry-90", Mat4::from_rotation_y(-1.5708)), ("Rx+90", Mat4::from_rotation_x(1.5708)), ("Rx-90", Mat4::from_rotation_x(-1.5708))] {
        let m = base * r;
        let hilt = m.transform_point3(Vec3::ZERO);
        let tip = m.transform_point3(Vec3::new(0.0, 0.0, 65.0));
        let tipx = m.transform_point3(Vec3::new(65.0, 0.0, 0.0));
        println!("{label:6}: hilt {:?} tip(+Z) up {:.0} right {:.0} fwd {:.0} | tip(+X) up {:.0} right {:.0} fwd {:.0}", hilt.to_array().map(|v| v.round()), tip.x, tip.y, -tip.z, tipx.x, tipx.y, -tipx.z);
    }
    let a = |v: i32| v as f32 / 65536.0 * std::f32::consts::TAU;
    let socket = Mat4::from_rotation_z(a(16384));
    for roll_sign in [1.0f32, -1.0] {
        let ro = Mat4::from_rotation_z(a(-16384)) * Mat4::from_rotation_x(roll_sign * a(16384));
        let m = base * socket * ro;
        let tip = m.transform_point3(Vec3::new(0.0, 0.0, 65.0));
        println!("socket*RotOrigin roll sign {roll_sign}: tip up {:.0} right {:.0} fwd {:.0}", tip.x, tip.y, -tip.z);
    }
    println!("camera_jnt world pos {:?}, axes x {:?} y {:?} z {:?}", world[cam].w_axis.truncate(), world[cam].x_axis.truncate(), world[cam].y_axis.truncate(), world[cam].z_axis.truncate());
    let _ = Quat::IDENTITY; let _ = Vec3::ZERO;
}
