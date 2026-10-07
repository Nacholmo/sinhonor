//! Dev tool: resolve an event in a package from your install and decode its clips.
//! usage: cargo run -p wwise --example wwise_event -- <Bank.pck> <EventName> [out_dir]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = wwise::Package::open(&a[0]).expect("open package");
    let node = pkg.event(&a[1]).expect("event not found");
    println!("{node:?}");
    for id in node.clips() {
        let ogg = pkg.ogg(id).expect("decode");
        let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(&ogg)).expect("ogg");
        let (rate, ch) = (r.ident_hdr.audio_sample_rate, r.ident_hdr.audio_channels);
        let mut samples = 0usize;
        while let Ok(Some(p)) = r.read_dec_packet_itl() {
            samples += p.len();
        }
        println!("  {id:#010x}: {} bytes ogg, {rate} Hz x{ch}, {:.2}s", ogg.len(), samples as f32 / ch as f32 / rate as f32);
        if let Some(dir) = a.get(2) {
            std::fs::write(format!("{dir}/{id:08x}.ogg"), &ogg).unwrap();
        }
    }
}
