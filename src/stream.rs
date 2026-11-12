//! Filtre du flux PTY : extrait les séquences gérées par NovaTerm (que la
//! bibliothèque d'émulation ignore) sans copier inutilement les grosses charges
//! utiles (images de plusieurs Mo envoyées en plusieurs lectures).

use memchr::{memchr, memchr2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqKind {
    /// Intégration shell NovaTerm (OSC 6973).
    Nova,
    /// Image en ligne iTerm2 (OSC 1337;File=…).
    ITerm,
    /// Notification (OSC 777;notify;titre;texte).
    Notify777,
    /// Notification simple (OSC 9;texte).
    Notify9,
    /// Protocole graphique Kitty (APC G…).
    Kitty,
    /// Dossier courant (OSC 7;file://hôte/chemin).
    Cwd,
}

const PREFIXES: &[(&[u8], SeqKind, usize)] = &[
    (b"\x1b]6973;", SeqKind::Nova, 256 * 1024),
    (b"\x1b]1337;", SeqKind::ITerm, 96 * 1024 * 1024),
    (b"\x1b]777;", SeqKind::Notify777, 64 * 1024),
    (b"\x1b]9;", SeqKind::Notify9, 64 * 1024),
    (b"\x1b_G", SeqKind::Kitty, 96 * 1024 * 1024),
    (b"\x1b]7;", SeqKind::Cwd, 8 * 1024),
];

#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    Data(Vec<u8>),
    Seq(SeqKind, Vec<u8>),
}

struct InSeq {
    kind: SeqKind,
    body: Vec<u8>,
    max: usize,
    esc_pending: bool,
    overflow: bool,
}

#[derive(Default)]
pub struct SeqFilter {
    /// Début de séquence coupé en fin de lecture (quelques octets au plus).
    partial: Vec<u8>,
    seq: Option<InSeq>,
}

fn match_prefix(rest: &[u8]) -> Result<Option<(usize, SeqKind, usize)>, ()> {
    let mut partial = false;
    for (p, kind, max) in PREFIXES {
        if rest.len() >= p.len() {
            if rest.starts_with(p) {
                return Ok(Some((p.len(), *kind, *max)));
            }
        } else if p.starts_with(rest) {
            partial = true;
        }
    }
    if partial {
        Err(())
    } else {
        Ok(None)
    }
}

impl SeqFilter {
    pub fn feed(&mut self, input: &[u8]) -> Vec<Chunk> {
        let owned;
        let data: &[u8] = if self.partial.is_empty() {
            input
        } else {
            let mut v = std::mem::take(&mut self.partial);
            v.extend_from_slice(input);
            owned = v;
            &owned
        };
        let mut out = Vec::new();
        let mut i = 0;
        let mut plain_start = 0;
        while i < data.len() {
            if let Some(seq) = self.seq.as_mut() {
                let mut finished = false;
                if seq.esc_pending {
                    seq.esc_pending = false;
                    if data[i] == b'\\' {
                        i += 1;
                        finished = true;
                    } else if !seq.overflow {
                        seq.body.push(0x1b);
                    }
                }
                if !finished {
                    match memchr2(0x07, 0x1b, &data[i..]) {
                        None => {
                            if !seq.overflow {
                                seq.body.extend_from_slice(&data[i..]);
                            }
                            i = data.len();
                        }
                        Some(off) => {
                            if !seq.overflow {
                                seq.body.extend_from_slice(&data[i..i + off]);
                            }
                            let b = data[i + off];
                            i += off + 1;
                            if b == 0x07 {
                                finished = true;
                            } else if i < data.len() {
                                if data[i] == b'\\' {
                                    i += 1;
                                    finished = true;
                                } else if !seq.overflow {
                                    seq.body.push(0x1b);
                                }
                            } else {
                                seq.esc_pending = true;
                            }
                        }
                    }
                }
                if seq.body.len() > seq.max {
                    seq.overflow = true;
                    seq.body = Vec::new();
                }
                if finished {
                    let seq = self.seq.take().expect("séquence en cours");
                    if !seq.overflow {
                        out.push(Chunk::Seq(seq.kind, seq.body));
                    }
                    plain_start = i;
                }
                continue;
            }
            let Some(off) = memchr(0x1b, &data[i..]) else { break };
            let e = i + off;
            match match_prefix(&data[e..]) {
                Ok(Some((plen, kind, max))) => {
                    if e > plain_start {
                        out.push(Chunk::Data(data[plain_start..e].to_vec()));
                    }
                    self.seq = Some(InSeq { kind, body: Vec::new(), max, esc_pending: false, overflow: false });
                    i = e + plen;
                    plain_start = i;
                }
                Err(()) => {
                    if e > plain_start {
                        out.push(Chunk::Data(data[plain_start..e].to_vec()));
                    }
                    self.partial = data[e..].to_vec();
                    return out;
                }
                Ok(None) => i = e + 1,
            }
        }
        if self.seq.is_none() && plain_start < data.len() {
            out.push(Chunk::Data(data[plain_start..].to_vec()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(f: &mut SeqFilter, parts: &[&[u8]]) -> (Vec<u8>, Vec<(SeqKind, Vec<u8>)>) {
        let mut data = Vec::new();
        let mut seqs = Vec::new();
        for p in parts {
            for c in f.feed(p) {
                match c {
                    Chunk::Data(d) => data.extend(d),
                    Chunk::Seq(k, b) => seqs.push((k, b)),
                }
            }
        }
        (data, seqs)
    }

    #[test]
    fn extracts_sequences() {
        let mut f = SeqFilter::default();
        let (d, s) = collect(&mut f, &[b"ab\x1b]6973;L;1;x\x07cd\x1b_Ga=T;QUJD\x1b\\e"]);
        assert_eq!(d, b"abcde");
        assert_eq!(s, vec![(SeqKind::Nova, b"L;1;x".to_vec()), (SeqKind::Kitty, b"a=T;QUJD".to_vec())]);
    }

    #[test]
    fn split_everywhere() {
        let input = b"x\x1b]1337;File=inline=1:AAAA\x1b\\y\x1b]0;title\x07z";
        for cut in 0..input.len() {
            let mut f = SeqFilter::default();
            let (d, s) = collect(&mut f, &[&input[..cut], &input[cut..]]);
            assert_eq!(d, b"xy\x1b]0;title\x07z", "coupure {cut}");
            assert_eq!(s, vec![(SeqKind::ITerm, b"File=inline=1:AAAA".to_vec())], "coupure {cut}");
        }
    }

    #[test]
    fn other_escapes_pass_through() {
        let mut f = SeqFilter::default();
        let (d, s) = collect(&mut f, &[b"\x1b[31mrouge\x1b[0m \x1b]8;;http://a\x1b\\lien\x1b]8;;\x1b\\"]);
        assert!(s.is_empty());
        assert_eq!(d, b"\x1b[31mrouge\x1b[0m \x1b]8;;http://a\x1b\\lien\x1b]8;;\x1b\\");
    }
}
