//! Decoder for Sony "Edge" animation data as cooked into Dishonored's `AnimSequence`s
//! (`CompressedByteStream`, tag `"50AE"`) and `SkeletalMesh`es (`m_EdgeSkeleton`, tag `"30SE"`).
//!
//! Written from the format description in NOTES.md (itself summarised from public
//! documentation of the Edge SDK layout and Dishonored research notes). Headers, tables, packing
//! specs and frame-set sizes are little-endian; key data is a big-endian bit stream.
//! Offsets in headers are self-relative (relative to the field that holds them).

use glam::{Quat, Vec3};

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "edge anim: {}", self.0)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn u16_at(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| Error(format!("truncated at {o}")))
}
fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| Error(format!("truncated at {o}")))
}
fn f32_at(d: &[u8], o: usize) -> Result<f32> {
    u32_at(d, o).map(f32::from_bits)
}
/// Resolves a self-relative offset stored at `field` (0 = absent).
fn rel(d: &[u8], field: usize) -> Result<Option<usize>> {
    let v = u32_at(d, field)? as usize;
    Ok((v != 0).then_some(field + v))
}

/// A joint's local transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub rotation: Quat,
    pub translation: Vec3,
}

/// The Edge skeleton stored on a skeletal mesh. Joint `i` is the mesh's bone `i`.
#[derive(Clone, Debug)]
pub struct Skeleton {
    pub base_pose: Vec<Joint>,
    pub name_hashes: Vec<u32>,
    pub parents: Vec<i16>,
}

pub const SKELETON_TAG: u32 = 0x4553_3033; // "30SE"
pub const ANIMATION_TAG: u32 = 0x4541_3035; // "50AE"

impl Skeleton {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if u32_at(d, 0)? != SKELETON_TAG {
            return Err(Error("not an Edge skeleton".into()));
        }
        let n = u16_at(d, 16)? as usize;
        let base = rel(d, 24)?.ok_or_else(|| Error("no base pose".into()))?;
        let parents_at = rel(d, 28)?.ok_or_else(|| Error("no parents".into()))?;
        let hashes_at = rel(d, 32)?.ok_or_else(|| Error("no name hashes".into()))?;
        let mut base_pose = Vec::with_capacity(n);
        let mut name_hashes = Vec::with_capacity(n);
        let mut parents = Vec::with_capacity(n);
        for i in 0..n {
            let o = base + i * 48;
            let q = Quat::from_xyzw(f32_at(d, o)?, f32_at(d, o + 4)?, f32_at(d, o + 8)?, f32_at(d, o + 12)?);
            let t = Vec3::new(f32_at(d, o + 16)?, f32_at(d, o + 20)?, f32_at(d, o + 24)?);
            base_pose.push(Joint { rotation: q, translation: t });
            name_hashes.push(u32_at(d, hashes_at + i * 4)?);
            parents.push(u16_at(d, parents_at + i * 2)? as i16);
        }
        Ok(Self { base_pose, name_hashes, parents })
    }

    pub fn joint_by_hash(&self, h: u32) -> Option<usize> {
        self.name_hashes.iter().position(|&x| x == h)
    }
}

// Header field offsets.
const H_DURATION: usize = 4;
const H_FREQ: usize = 8;
// sizeHeader is a u16 at 12; the u16 counts follow it.
const H_NUM_JOINTS: usize = 14;
const H_NUM_FRAME_SETS: usize = 18;
const H_CONST_R: usize = 22;
const H_ANIM_R: usize = 30;
const H_FLAGS: usize = 38;
const H_OFF_DMA: usize = 56;
const H_OFF_INFO: usize = 60;
const H_OFF_CONST_R: usize = 64;
const H_OFF_PACKING: usize = 80;
const H_OFF_CUSTOM: usize = 84;
const H_OFF_LOCO: usize = 92;
const H_TABLES: usize = 96;

/// One cooked Edge animation.
#[derive(Clone, Debug)]
pub struct Animation {
    blob: Vec<u8>,
    pub duration: f32,
    pub sample_frequency: f32,
    pub num_joints: usize,
    /// Animation joint -> joint name hash.
    pub joint_hashes: Vec<u32>,
    flags: u16,
    /// Channel counts: const R/T/S/User, anim R/T/S/User.
    num_const: [usize; 4],
    num_anim: [usize; 4],
    /// Channel tables (animation joint index per channel), same order as the counts.
    tables_const: [Vec<u16>; 4],
    tables_anim: [Vec<u16>; 4],
}

/// Reads a big-endian bit stream.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.d.get(self.pos / 8).copied().unwrap_or(0);
            let bit = (byte >> (7 - (self.pos % 8))) & 1;
            v = (v << 1) | bit as u32;
            self.pos += 1;
        }
        v
    }
}

/// Per-component (sign, exponent, mantissa) bit counts from a packing spec, plus the omitted
/// quaternion slot.
#[derive(Clone, Copy, Debug)]
struct Spec {
    comp: [(u32, u32, u32); 3],
    omitted: usize,
}

impl Spec {
    fn new(v: u32) -> Self {
        let c = |sb: u32, eb: u32, mb: u32| ((v >> sb) & 1, (v >> eb) & 0xF, (v >> mb) & 0x1F);
        Spec { comp: [c(31, 27, 22), c(21, 17, 12), c(11, 7, 2)], omitted: (v & 3) as usize }
    }
    fn key_bits(&self) -> u32 {
        self.comp.iter().map(|(s, e, m)| s + e + m).sum()
    }
    fn decode(&self, bits: &mut Bits) -> [f32; 3] {
        let mut out = [0.0; 3];
        for (k, &(sb, eb, mb)) in self.comp.iter().enumerate() {
            let s = bits.read(sb);
            let e = bits.read(eb);
            let m = bits.read(mb);
            out[k] = if eb == 0 {
                if mb == 0 {
                    0.0
                } else {
                    let raw = if s != 0 { (m | (!0u32 << mb)) as i32 } else { m as i32 };
                    raw as f32 / ((1u64 << mb) - 1) as f32
                }
            } else {
                let exp = e as i32 + 128 - (1 << (eb - 1));
                let mant = if mb >= 32 { 0 } else { m << (23 - mb.min(23)) };
                f32::from_bits(s << 31 | ((exp as u32) & 0xFF) << 23 | mant)
            };
        }
        out
    }
}

fn quat_from_three(v: [f32; 3], omitted: usize) -> Quat {
    let mut q = [0.0f32; 4];
    let mut k = 0;
    for (slot, out) in q.iter_mut().enumerate() {
        if slot != omitted {
            *out = v[k];
            k += 1;
        }
    }
    let rest = 1.0 - v[0] * v[0] - v[1] * v[1] - v[2] * v[2];
    q[omitted] = rest.clamp(0.0, 1.0).sqrt();
    Quat::from_xyzw(q[0], q[1], q[2], q[3])
}

fn quat48(d: &[u8], o: usize) -> Quat {
    let a = u16::from_be_bytes([d[o], d[o + 1]]) as u32;
    let rest = u32::from_be_bytes([d[o + 2], d[o + 3], d[o + 4], d[o + 5]]);
    let b = rest >> 17;
    let c = (rest >> 2) & 0x7FFF;
    let omitted = (rest & 3) as usize;
    let f = |v: u32| v as f32 * 4.3159689e-05 - std::f32::consts::FRAC_1_SQRT_2;
    quat_from_three([f(a & 0x7FFF), f(b), f(c)], omitted)
}

fn align(x: usize, a: usize) -> usize {
    x.div_ceil(a) * a
}

impl Animation {
    pub fn parse(blob: &[u8]) -> Result<Self> {
        if u32_at(blob, 0)? != ANIMATION_TAG {
            return Err(Error("not an Edge animation".into()));
        }
        let d = blob;
        let count = |o: usize| -> Result<[usize; 4]> { Ok([u16_at(d, o)? as usize, u16_at(d, o + 2)? as usize, u16_at(d, o + 4)? as usize, u16_at(d, o + 6)? as usize]) };
        let num_const = count(H_CONST_R)?;
        let num_anim = count(H_ANIM_R)?;
        let num_joints = u16_at(d, H_NUM_JOINTS)? as usize;
        let mut o = H_TABLES;
        let mut take = |n: usize, pad: usize| -> Result<Vec<u16>> {
            let v = (0..n).map(|i| u16_at(d, o + 2 * i)).collect::<Result<Vec<_>>>()?;
            o += 2 * align(n, pad);
            Ok(v)
        };
        let tables_const = [take(num_const[0], 8)?, take(num_const[1], 4)?, take(num_const[2], 4)?, take(num_const[3], 4)?];
        let tables_anim = [take(num_anim[0], 4)?, take(num_anim[1], 4)?, take(num_anim[2], 4)?, take(num_anim[3], 4)?];
        let mut joint_hashes = Vec::with_capacity(num_joints);
        if let Some(custom) = rel(d, H_OFF_CUSTOM)? {
            let skip = if u32_at(d, H_OFF_LOCO)? != 0 { 32 } else { 0 };
            for i in 0..num_joints {
                joint_hashes.push(u32_at(d, custom + skip + 4 * i)?);
            }
        }
        Ok(Self {
            blob: blob.to_vec(),
            duration: f32_at(d, H_DURATION)?,
            sample_frequency: f32_at(d, H_FREQ)?,
            num_joints,
            joint_hashes,
            flags: u16_at(d, H_FLAGS)?,
            num_const,
            num_anim,
            tables_const,
            tables_anim,
        })
    }

    /// Evaluates the animation at `time` seconds onto `skeleton`'s base pose. Joints without a
    /// channel keep the base pose (or identity for `additive` sequences).
    pub fn evaluate(&self, skeleton: &Skeleton, time: f32, additive: bool) -> Result<Vec<Joint>> {
        let d = &self.blob;
        let mut pose: Vec<Joint> = if additive {
            vec![Joint { rotation: Quat::IDENTITY, translation: Vec3::ZERO }; skeleton.base_pose.len()]
        } else {
            skeleton.base_pose.clone()
        };
        // Animation joint -> skeleton joint.
        let map: Vec<Option<usize>> = (0..self.num_joints)
            .map(|j| self.joint_hashes.get(j).and_then(|&h| skeleton.joint_by_hash(h)))
            .collect();
        let set_r = |aj: u16, q: Quat, pose: &mut Vec<Joint>| {
            if let Some(Some(s)) = map.get(aj as usize) {
                pose[*s].rotation = q;
            }
        };

        // Frame set lookup.
        let frame = (time * self.sample_frequency).max(0.0);
        let sets = u16_at(d, H_NUM_FRAME_SETS)? as usize;
        let info = rel(d, H_OFF_INFO)?.ok_or_else(|| Error("no frame set info".into()))?;
        let dma = rel(d, H_OFF_DMA)?.ok_or_else(|| Error("no frame set table".into()))?;
        let mut k = 0;
        for i in 0..sets.saturating_sub(1).max(1) {
            if (u16_at(d, info + 4 * i)? as f32) <= frame.min(65535.0).floor() {
                k = i;
            }
        }
        let base_frame = u16_at(d, info + 4 * k)? as f32;
        let num_intra = u16_at(d, info + 4 * k + 2)? as usize;
        let fs_size = u16_at(d, dma + 8 * k + 2)? as usize;
        let fs = u32_at(d, dma + 8 * k + 4)? as usize;
        let _ = fs_size;
        let rel_frame = frame - base_frame;
        let (mut fi, mut ff) = (rel_frame.floor() as usize, rel_frame.fract());
        if fi > num_intra {
            fi = num_intra;
            ff = 1.0;
        }

        // Packing specs: per kind, one for the constant channels then one per animated channel.
        let specs_at = rel(d, H_OFF_PACKING)?;
        let mut spec_cursor = specs_at.unwrap_or(0);
        let mut next_spec = || -> Result<Spec> {
            let s = Spec::new(u32_at(d, spec_cursor)?);
            spec_cursor += 4;
            Ok(s)
        };
        let bitpacked = [self.flags & 1 != 0, self.flags & 2 != 0, self.flags & 4 != 0];

        // Constant channels (R, T, S).
        let mut const_specs = [None; 3];
        let mut anim_specs: [Vec<Spec>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for kind in 0..3 {
            if bitpacked[kind] {
                const_specs[kind] = Some(next_spec()?);
                anim_specs[kind] = (0..self.num_anim[kind]).map(|_| next_spec()).collect::<Result<_>>()?;
            }
        }
        for kind in 0..3 {
            let n = self.num_const[kind];
            if n == 0 {
                continue;
            }
            let at = rel(d, H_OFF_CONST_R + 4 * kind)?.ok_or_else(|| Error("missing const data".into()))?;
            if bitpacked[kind] {
                let spec = const_specs[kind].unwrap();
                let mut bits = Bits { d: &d[at..], pos: 0 };
                for c in 0..n {
                    let v = spec.decode(&mut bits);
                    let aj = self.tables_const[kind][c];
                    match kind {
                        0 => set_r(aj, quat_from_three(v, spec.omitted), &mut pose),
                        1 => {
                            if let Some(Some(s)) = map.get(aj as usize) {
                                pose[*s].translation = Vec3::from(v);
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                for c in 0..n {
                    let aj = self.tables_const[kind][c];
                    match kind {
                        0 => set_r(aj, quat48(d, at + 6 * c), &mut pose),
                        1 => {
                            if let Some(Some(s)) = map.get(aj as usize) {
                                pose[*s].translation = Vec3::new(f32_at(d, at + 12 * c)?, f32_at(d, at + 12 * c + 4)?, f32_at(d, at + 12 * c + 8)?);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Animated channels from the frame set.
        let mut o = fs;
        let sizes: Vec<usize> = (0..8).map(|i| u16_at(d, o + 2 * i).map(|v| v as usize)).collect::<Result<_>>()?;
        o += 16;
        let initial = [o, o + sizes[0], o + sizes[0] + sizes[1], o + sizes[0] + sizes[1] + sizes[2]];
        o = initial[3] + sizes[3];
        let total_anim: usize = self.num_anim.iter().sum();
        let intra_bits_at = o;
        o += (num_intra * total_anim).div_ceil(8);
        let intra = [o, o + sizes[4], o + sizes[4] + sizes[5]];
        o = intra[2] + sizes[6];
        o = align(o, 4) + sizes[7];
        o = align(o, 16);
        let fsizes: Vec<usize> = (0..8).map(|i| u16_at(d, o + 2 * i).map(|v| v as usize)).collect::<Result<_>>()?;
        o += 16;
        let fin = [o, o + fsizes[0], o + fsizes[0] + fsizes[1]];

        let intra_bit = |ch_global: usize, kf: usize| -> bool {
            let b = ch_global * num_intra + kf;
            d.get(intra_bits_at + b / 8).is_some_and(|&byte| byte >> (7 - (b % 8)) & 1 != 0)
        };

        let mut ch_base = 0;
        for kind in 0..2 {
            let n = self.num_anim[kind];
            let mut init_bits = 0u32;
            let mut intra_bits = 0u32;
            for c in 0..n {
                let g = ch_base + c;
                let spec_bits;
                let read_key = |stream_at: usize, bit_off: u32, spec: Option<&Spec>| -> Result<(Quat, Vec3)> {
                    if let Some(sp) = spec {
                        let mut b = Bits { d: &d[stream_at..], pos: bit_off as usize };
                        let v = sp.decode(&mut b);
                        Ok((quat_from_three(v, sp.omitted), Vec3::from(v)))
                    } else if kind == 0 {
                        Ok((quat48(d, stream_at + bit_off as usize / 8), Vec3::ZERO))
                    } else {
                        let p = stream_at + bit_off as usize / 8;
                        Ok((Quat::IDENTITY, Vec3::new(f32_at(d, p)?, f32_at(d, p + 4)?, f32_at(d, p + 8)?)))
                    }
                };
                let spec = anim_specs[kind].get(c);
                spec_bits = match spec {
                    Some(s) => s.key_bits(),
                    None => {
                        if kind == 0 {
                            48
                        } else {
                            96
                        }
                    }
                };
                // Keys present in this frame set for the channel: initial (frame 0), intra keys
                // where the bit is set (frame k+1), final (frame num_intra + 1).
                let keys: Vec<usize> = (0..num_intra).filter(|&kf| intra_bit(g, kf)).collect();
                let left = keys.iter().rev().find(|&&kf| kf < fi).copied();
                let right = keys.iter().find(|&&kf| kf >= fi).copied();
                let key_at = |which: Option<usize>, is_final: bool| -> Result<(Quat, Vec3, usize)> {
                    match which {
                        Some(kf) => {
                            let idx = keys.iter().position(|&x| x == kf).unwrap() as u32;
                            let (q, t) = read_key(intra[kind], intra_bits + idx * spec_bits, spec)?;
                            Ok((q, t, kf + 1))
                        }
                        None if is_final => {
                            let (q, t) = read_key(fin[kind], init_bits, spec)?;
                            Ok((q, t, num_intra + 1))
                        }
                        None => {
                            let (q, t) = read_key(initial[kind], init_bits, spec)?;
                            Ok((q, t, 0))
                        }
                    }
                };
                let (lq, lt, lf) = key_at(left, false)?;
                let (rq, rt, rf) = key_at(right, true)?;
                let denom = (rf as f32 - fi as f32) + (fi as f32 - lf as f32);
                let t = if denom > 0.0 { ((fi as f32 - lf as f32) + ff) / denom } else { 0.0 };
                let aj = self.tables_anim[kind][c];
                if let Some(Some(s)) = map.get(aj as usize) {
                    if kind == 0 {
                        let rq = if lq.dot(rq) < 0.0 { -rq } else { rq };
                        pose[*s].rotation = lq.slerp(rq, t.clamp(0.0, 1.0));
                    } else {
                        pose[*s].translation = lt.lerp(rt, t.clamp(0.0, 1.0));
                    }
                }
                init_bits += spec_bits;
                intra_bits += keys.len() as u32 * spec_bits;
            }
            ch_base += n;
        }
        Ok(pose)
    }
}
