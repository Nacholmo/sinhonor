//! Dev check: decode an animation from your install against the arms' Edge skeleton.
use edge_anim::{Animation, Skeleton};
use glam::{Quat, Vec3};
use upk::{lookup, Package, Value};

fn world(pose: &[(Quat, Vec3)], parents: &[usize]) -> Vec<(Quat, Vec3)> {
    let mut w: Vec<(Quat, Vec3)> = Vec::with_capacity(pose.len());
    for (i, &(q, t)) in pose.iter().enumerate() {
        if i == 0 || parents[i] == i {
            w.push((q, t));
        } else {
            let (pq, pt) = w[parents[i]];
            w.push((pq * q, pt + pq * t));
        }
    }
    w
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::Path::new(&a[0]);
    let engine = Package::open(dir.join("Engine.upk")).unwrap();
    let mesh = engine.skeletal_mesh(engine.find_export("Ply_Player.Skm_Player").unwrap()).unwrap();
    let skel = Skeleton::parse(&mesh.edge_skeleton).unwrap();
    let parents: Vec<usize> = mesh.bones.iter().map(|b| b.parent).collect();
    // Base pose vs RefSkeleton.
    let b = &mesh.bones[54];
    println!("hand_L ref q {:?} t {:?}\n       edge q {:?} t {:?}", b.orientation, b.position, skel.base_pose[54].rotation, skel.base_pose[54].translation);
    // Which W convention puts hand_L near its vertices?
    let hand_verts: Vec<Vec3> = mesh.vertices.iter().filter(|v| v.bones[0] == 54 && v.weights[0] > 0.9).map(|v| Vec3::from(v.position)).collect();
    let centroid = hand_verts.iter().copied().sum::<Vec3>() / hand_verts.len().max(1) as f32;
    for flip in [false, true] {
        let pose: Vec<(Quat, Vec3)> = mesh.bones.iter().enumerate().map(|(i, b)| {
            let mut q = Quat::from_xyzw(b.orientation[0], b.orientation[1], b.orientation[2], b.orientation[3]);
            if flip && i > 0 { q = q.conjugate(); }
            (q, Vec3::from(b.position))
        }).collect();
        let w = world(&pose, &parents);
        println!("flip {flip}: hand_L world {:?}, hand verts centroid {centroid:?} ({} verts), dist {:.1}", w[54].1, hand_verts.len(), w[54].1.distance(centroid));
    }
    // Decode an animation.
    let startup = Package::open(dir.join("Startup.upk")).unwrap();
    for i in startup.exports_of_class("AnimSequence") {
        let Ok((p, tail)) = startup.properties_and_tail(i) else { continue };
        let Some(Value::Name(n)) = lookup(&p, "SequenceName") else { continue };
        if n != &a[1] { continue }
        let d = startup.export_data(i);
        let len = u32::from_le_bytes(d[tail + 4..tail + 8].try_into().unwrap()) as usize;
        let blob = &d[tail + 8..tail + 8 + len];
        if let Ok(out) = std::env::var("DUMP") { std::fs::write(out, blob).unwrap(); }
        let anim = Animation::parse(blob).unwrap();
        let mapped = anim.joint_hashes.iter().filter(|h| skel.joint_by_hash(**h).is_some()).count();
        println!("{n}: {} joints ({} mapped), {:.2}s @ {:.1} Hz", anim.num_joints, mapped, anim.duration, anim.sample_frequency);
        let names: Vec<&str> = anim.joint_hashes.iter().filter_map(|h| skel.joint_by_hash(*h)).map(|j| mesh.bones[j].name.as_str()).collect();
        println!("  joints: {}", names.join(" "));
        for t in [0.0, anim.duration * 0.5, anim.duration] {
            let pose = anim.evaluate(&skel, t, false).unwrap();
            let bad = pose.iter().filter(|j| (j.rotation.length() - 1.0).abs() > 0.01 || !j.translation.is_finite()).count();
            println!("  t={t:.2}: non-unit/non-finite joints {bad}; hand_L q {:?}", pose[54].rotation);
        }
    }
}
