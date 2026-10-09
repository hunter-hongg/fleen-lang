//! `.flnc` binary serialization (BYTECODE.md §9).

use super::bytecode::*;

const MAGIC: &[u8; 4] = b"FLNC";

/// Versions this reader accepts: 0.0.1 (v1) and 0.0.2 (v2) modules.
///
/// v2 adds instructions and fills `span_map`; the field order and widths of
/// every existing table are unchanged (BYTECODE.md §9), so a v1 file reads
/// unmodified.
const READ_VERSIONS: [u16; 2] = [1, 2];

/// Error produced by `from_bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlncError(pub String);

impl std::fmt::Display for FlncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "flnc error: {}", self.0)
    }
}

impl std::error::Error for FlncError {}

struct W(Vec<u8>);
impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
}

struct R<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FlncError> {
        if self.pos + n > self.buf.len() {
            return Err(FlncError("truncated input".into()));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, FlncError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FlncError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, FlncError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, FlncError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, FlncError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

/// Serialize a module to the `.flnc` byte format.
///
/// Writes `module.version` verbatim, so a reader that round-trips a module
/// preserves its version.
pub fn to_bytes(m: &Module) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.bytes(MAGIC);
    w.u16(m.version);

    w.u32(m.constants.len() as u32);
    for c in &m.constants {
        match c {
            Const::Int(v) => {
                w.u8(0);
                w.i64(*v);
            }
            Const::Float(v) => {
                w.u8(1);
                w.u64(v.to_bits());
            }
            Const::Str(s) => {
                w.u8(2);
                w.u32(s.len() as u32);
                w.bytes(s.as_bytes());
            }
        }
    }

    w.u32(m.functions.len() as u32);
    for f in &m.functions {
        w.u32(f.name.0);
        w.u16(f.params);
        w.u16(f.locals);
        w.u8(u8::from(f.is_builtin));
        w.u32(f.code.len() as u32);
        w.bytes(&f.code);
        w.u32(f.span_map.len() as u32);
        for e in f.span_map.iter() {
            w.u32(e.offset);
            w.u32(e.start);
            w.u32(e.end);
        }
    }

    w.u32(m.globals.len() as u32);
    for g in &m.globals {
        w.u32(g.name.0);
        w.u8(u8::from(g.mutable));
    }

    w.u32(m.entry.0);
    w.0
}

/// Deserialize a `.flnc` byte stream into a module.
pub fn from_bytes(buf: &[u8]) -> Result<Module, FlncError> {
    let mut r = R { buf, pos: 0 };
    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(FlncError("bad magic".into()));
    }
    let version = r.u16()?;
    if !READ_VERSIONS.contains(&version) {
        return Err(FlncError(format!("unsupported version {version}")));
    }

    let nc = r.u32()? as usize;
    let mut constants = Vec::with_capacity(nc);
    for _ in 0..nc {
        match r.u8()? {
            0 => constants.push(Const::Int(r.i64()?)),
            1 => constants.push(Const::Float(f64::from_bits(r.u64()?))),
            2 => {
                let len = r.u32()? as usize;
                let bytes = r.take(len)?;
                let s = std::str::from_utf8(bytes)
                    .map_err(|_| FlncError("invalid utf8 in string constant".into()))?;
                constants.push(Const::Str(s.into()));
            }
            t => return Err(FlncError(format!("bad const tag {t}"))),
        }
    }

    let nf = r.u32()? as usize;
    let mut functions = Vec::with_capacity(nf);
    for _ in 0..nf {
        let name = ConstId(r.u32()?);
        if name.0 as usize >= nc {
            return Err(FlncError("func name ConstId out of range".into()));
        }
        let params = r.u16()?;
        let locals = r.u16()?;
        let is_builtin = match r.u8()? {
            0 => false,
            1 => true,
            v => return Err(FlncError(format!("bad is_builtin byte {v}"))),
        };
        let code_len = r.u32()? as usize;
        let code = r.take(code_len)?.to_vec().into_boxed_slice();
        let ns = r.u32()? as usize;
        let mut span_map = Vec::with_capacity(ns);
        for _ in 0..ns {
            span_map.push(SpanEntry {
                offset: r.u32()?,
                start: r.u32()?,
                end: r.u32()?,
            });
        }
        functions.push(Func {
            name,
            params,
            locals,
            code,
            span_map: span_map.into_boxed_slice(),
            is_builtin,
        });
    }

    let ng = r.u32()? as usize;
    let mut globals = Vec::with_capacity(ng);
    for _ in 0..ng {
        let name = ConstId(r.u32()?);
        if name.0 as usize >= nc {
            return Err(FlncError("global name ConstId out of range".into()));
        }
        let mutable = match r.u8()? {
            0 => false,
            1 => true,
            v => return Err(FlncError(format!("bad mutable byte {v}"))),
        };
        globals.push(Global { name, mutable });
    }

    let entry = FuncId(r.u32()?);
    if entry.0 as usize >= nf {
        return Err(FlncError("entry FuncId out of range".into()));
    }
    if r.pos != buf.len() {
        return Err(FlncError("trailing bytes".into()));
    }

    Ok(Module {
        version,
        constants,
        functions,
        globals,
        entry,
    })
}
