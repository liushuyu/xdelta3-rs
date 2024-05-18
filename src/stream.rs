use futures_io::*;
use futures_util::io::*;
use std::collections::BTreeMap;
use std::io;

use super::binding;
use log::{debug, trace};

pub use binding::xd3_smatch_cfg;

#[allow(unused)]
const XD3_DEFAULT_WINSIZE: usize = 1 << 23;
const XD3_DEFAULT_SRCWINSZ: u64 = 1 << 26;
#[allow(unused)]
const XD3_DEFAULT_ALLOCSIZE: usize = 1 << 14;
#[allow(unused)]
const XD3_DEFAULT_SPREVSZ: usize = 1 << 18;

struct CacheEntry {
    len: usize,
    buf: Box<[u8]>,
}

struct SrcBuffer<R> {
    src: Box<binding::xd3_source>,
    read: R,
    read_len: usize,
    eof_known: bool,

    block_offset: usize,
    block_len: usize,
    cache: BTreeMap<usize, CacheEntry>,
}
unsafe impl<R> Send for SrcBuffer<R> {}

impl<R> SrcBuffer<R> {
    fn new(cfg: &Xd3Config, read: R) -> io::Result<Self> {
        let block_count = 32;
        let max_winsize = cfg.source_window_size;
        let blksize = max_winsize / block_count;

        let cache = BTreeMap::new();

        let mut src: Box<binding::xd3_source> = Box::new(unsafe { std::mem::zeroed() });
        src.blksize = blksize as u32;
        src.max_winsize = max_winsize;

        Ok(Self {
            src,
            read,
            read_len: 0,
            eof_known: false,

            block_offset: 0,
            block_len: blksize as usize,
            cache,
        })
    }
}

impl<R> SrcBuffer<R> {}

impl<R: io::Read> SrcBuffer<R> {
    fn fetch(&mut self) -> Result<()> {
        let mut buf = if self.cache.len() == self.block_offset + 1 {
            let mut key = 0usize;
            for (k, _v) in &self.cache {
                key = *k;
                break;
            }
            self.cache.remove(&key).unwrap().buf
        } else {
            let v = vec![0u8; self.block_len];
            v.into_boxed_slice()
        };

        let mut read_len = 0;

        while read_len != buf.len() {
            let len = self.read.read(&mut buf[read_len..])?;
            if len == 0 {
                self.eof_known = true;
                break;
            } else {
                read_len += len;
            }
        }

        let entry = CacheEntry { len: read_len, buf };
        self.cache.insert(self.block_offset, entry);
        self.block_offset += 1;
        Ok(())
    }

    fn getblk(&mut self) -> io::Result<()> {
        trace!(
            "getsrcblk: curblkno={}, getblkno={}",
            self.src.curblkno,
            self.src.getblkno,
        );

        let blkno = self.src.getblkno as usize;

        let entry = loop {
            match self.cache.get_mut(&blkno) {
                Some(entry) => break entry,
                None => {
                    if blkno < self.block_offset {
                        eprintln!("invalid blkno={}, offset={}", blkno, self.block_offset);
                        for (k, _v) in &self.cache {
                            eprintln!("key={:?}", k);
                        }
                        panic!("invalid blkno");
                    }

                    self.fetch()?;
                    continue;
                }
            }
        };

        let src = &mut self.src;
        let buf_len = entry.len;
        let data = &entry.buf[..buf_len];

        src.curblkno = src.getblkno;
        src.curblk = data.as_ptr();
        src.onblk = buf_len as u32;

        src.eof_known = self.eof_known as i32;
        if !self.eof_known {
            src.max_blkno = src.curblkno;
            src.onlastblk = src.onblk;
        } else {
            src.max_blkno = (self.block_offset - 1) as u64;
            src.onlastblk = (self.read_len % src.blksize as usize) as u32;
        }
        Ok(())
    }
}

impl<R: AsyncRead + Unpin> SrcBuffer<R> {
    async fn fetch_async(&mut self) -> Result<()> {
        let mut buf = if self.cache.len() == self.block_offset + 1 {
            let mut key = 0usize;
            for (k, _v) in &self.cache {
                key = *k;
                break;
            }
            self.cache.remove(&key).unwrap().buf
        } else {
            let v = vec![0u8; self.block_len];
            v.into_boxed_slice()
        };

        let mut read_len = 0;

        while read_len != buf.len() {
            let len = self.read.read(&mut buf[read_len..]).await?;
            if len == 0 {
                self.eof_known = true;
                break;
            } else {
                read_len += len;
            }
        }

        let entry = CacheEntry { len: read_len, buf };
        self.cache.insert(self.block_offset, entry);
        self.block_offset += 1;
        Ok(())
    }

    async fn getblk_async(&mut self) -> io::Result<()> {
        trace!(
            "getsrcblk: curblkno={}, getblkno={}",
            self.src.curblkno,
            self.src.getblkno,
        );

        let blkno = self.src.getblkno as usize;

        let entry = loop {
            match self.cache.get_mut(&blkno) {
                Some(entry) => break entry,
                None => {
                    if blkno < self.block_offset {
                        eprintln!("invalid blkno={}, offset={}", blkno, self.block_offset);
                        for (k, _v) in &self.cache {
                            eprintln!("key={:?}", k);
                        }
                        panic!("invalid blkno");
                    }

                    self.fetch_async().await?;
                    continue;
                }
            }
        };

        let src = &mut self.src;
        let buf_len = entry.len;
        let data = &entry.buf[..buf_len];

        src.curblkno = src.getblkno;
        src.curblk = data.as_ptr();
        src.onblk = buf_len as u32;

        src.eof_known = self.eof_known as i32;
        if !self.eof_known {
            src.max_blkno = src.curblkno;
            src.onlastblk = src.onblk;
        } else {
            src.max_blkno = (self.block_offset - 1) as u64;
            src.onlastblk = (self.read_len % src.blksize as usize) as u32;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Xd3Config {
    inner: Box<binding::xd3_config>,

    // source config
    source_window_size: u64,
}
unsafe impl Send for Xd3Config {}

impl Xd3Config {
    pub fn new() -> Self {
        let mut cfg: binding::xd3_config = unsafe { std::mem::zeroed() };
        cfg.winsize = XD3_DEFAULT_WINSIZE as u32;
        cfg.sprevsz = XD3_DEFAULT_SPREVSZ as u32;

        let config = Self {
            inner: Box::new(cfg),
            source_window_size: XD3_DEFAULT_SRCWINSZ,
        };
        config
    }

    pub fn window_size(mut self, winsize: u32) -> Self {
        let inner = self.inner.as_mut();
        inner.winsize = winsize.next_power_of_two();
        self
    }

    pub fn sprev_size(mut self, sprevsz: u32) -> Self {
        let inner = self.inner.as_mut();
        inner.sprevsz = sprevsz.next_power_of_two();
        self
    }

    pub fn source_window_size(mut self, source_window_size: u64) -> Self {
        self.source_window_size = source_window_size.next_power_of_two();
        self
    }

    pub fn no_compress(mut self, no_compress: bool) -> Self {
        let inner = self.inner.as_mut();
        if no_compress {
            inner.flags |= binding::xd3_flags::XD3_NOCOMPRESS as i32;
        } else {
            inner.flags &= !(binding::xd3_flags::XD3_NOCOMPRESS as i32);
        }
        self
    }

    pub fn set_smatch_config(mut self, smatch_cfg: binding::xd3_smatch_cfg) -> Self {
        let inner = self.inner.as_mut();
        inner.smatch_cfg = smatch_cfg;
        self
    }

    pub fn level(mut self, mut level: i32) -> Self {
        use binding::xd3_flags::*;

        if level < 0 {
            level = 0;
        }
        if level > 9 {
            level = 9;
        }
        let flags = (self.inner.flags & (!(XD3_COMPLEVEL_MASK as i32)))
            | (level << XD3_COMPLEVEL_SHIFT as i32);

        self.inner.flags = flags;
        self
    }
}

struct Xd3Stream {
    inner: Box<binding::xd3_stream>,
}
impl Xd3Stream {
    fn new() -> Self {
        let inner: binding::xd3_stream = unsafe { std::mem::zeroed() };
        return Self {
            inner: Box::new(inner),
        };
    }
}
impl Drop for Xd3Stream {
    fn drop(&mut self) {
        unsafe {
            binding::xd3_free_stream(self.inner.as_mut() as *mut _);
        }
    }
}
unsafe impl Send for Xd3Stream {}

pub async fn decode_async<R1, R2, W>(input: R1, src: R2, out: W) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let cfg = Xd3Config::new();
    process_async(cfg, ProcessMode::Decode, input, src, out).await
}

pub async fn encode_async<R1, R2, W>(input: R1, src: R2, out: W) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let cfg = Xd3Config::new();
    process_async(cfg, ProcessMode::Encode, input, src, out).await
}

#[derive(Debug, Clone, Copy)]
pub enum ProcessMode {
    Encode,
    Decode,
}

pub fn process<R1, R2, W>(
    cfg: Xd3Config,
    mode: ProcessMode,
    mut input: R1,
    src: R2,
    mut output: W,
) -> io::Result<()>
where
    R1: io::Read,
    R2: io::Read,
    W: io::Write,
{
    let mut state = ProcessState::new(cfg, src)?;

    use binding::xd3_rvalues::*;

    loop {
        let res = state.step(mode);
        debug!("step: mode={:?}, res={:?}", mode, res);
        match res {
            XD3_INPUT => {
                if state.eof {
                    break;
                }
                state.read_input(&mut input)?;
            }
            XD3_OUTPUT => {
                state.write_output(&mut output)?;
            }
            XD3_GETSRCBLK => {
                state.src_buf.getblk()?;
            }
            XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                // do nothing
            }
            XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
            | XD3_UNIMPLEMENTED => {
                return Err(io::Error::new(io::ErrorKind::Other, format!("{:?}", res)));
            }
        }
    }

    output.flush()
}

pub async fn process_async<R1, R2, W>(
    cfg: Xd3Config,
    mode: ProcessMode,
    mut input: R1,
    src: R2,
    mut output: W,
) -> io::Result<()>
where
    R1: AsyncRead + Unpin,
    R2: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut state = ProcessState::new(cfg, src)?;

    use binding::xd3_rvalues::*;

    loop {
        let res = state.step(mode);
        match res {
            XD3_INPUT => {
                if state.eof {
                    break;
                }
                state.read_input_async(&mut input).await?;
            }
            XD3_OUTPUT => {
                state.write_output_async(&mut output).await?;
            }
            XD3_GETSRCBLK => {
                state.src_buf.getblk_async().await?;
            }
            XD3_GOTHEADER | XD3_WINSTART | XD3_WINFINISH => {
                // do nothing
            }
            XD3_TOOFARBACK | XD3_INTERNAL | XD3_INVALID | XD3_INVALID_INPUT | XD3_NOSECOND
            | XD3_UNIMPLEMENTED => {
                return Err(io::Error::new(io::ErrorKind::Other, format!("{:?}", res)));
            }
        }
    }

    output.flush().await
}

struct ProcessState<R> {
    #[allow(unused)]
    cfg: Xd3Config,
    stream: Xd3Stream,
    src_buf: SrcBuffer<R>,

    input_buf: Vec<u8>,
    eof: bool,
}

impl<R> ProcessState<R> {
    fn new(mut cfg: Xd3Config, src: R) -> io::Result<Self> {
        // log::info!("ProcessState::new config={:?}", cfg);

        let mut stream = Xd3Stream::new();
        let stream0 = stream.inner.as_mut();

        let ret = unsafe { binding::xd3_config_stream(stream0, cfg.inner.as_mut()) };
        if ret != 0 {
            let err = if stream0.msg == std::ptr::null() {
                Error::new(io::ErrorKind::Other, "xd3_config_stream: null")
            } else {
                let msg = unsafe { std::ffi::CStr::from_ptr(stream0.msg) };

                Error::new(
                    io::ErrorKind::Other,
                    format!("xd3_config_stream: {:?}, flags={:0b}", msg, stream0.flags),
                )
            };
            return Err(err);
        }

        let mut src_buf = SrcBuffer::new(&cfg, src)?;
        let ret = unsafe { binding::xd3_set_source(stream0, src_buf.src.as_mut()) };
        if ret != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "xd3_set_source"));
        }

        let input_buf_size = stream0.winsize as usize;
        trace!("stream.winsize={}", input_buf_size);
        let mut input_buf = Vec::with_capacity(input_buf_size);
        input_buf.resize(input_buf_size, 0u8);

        Ok(Self {
            cfg,
            stream,
            src_buf,
            input_buf,
            eof: false,
        })
    }

    fn read_input<R2>(&mut self, mut input: R2) -> io::Result<()>
    where
        R2: io::Read,
    {
        let input_buf = &mut self.input_buf;

        let read_size = match input.read(input_buf) {
            Ok(n) => n,
            Err(_e) => {
                debug!("error on read: {:?}", _e);
                return Err(io::Error::new(io::ErrorKind::Other, "xd3: read_input"));
            }
        };
        debug!("read_size={}", read_size);

        {
            let stream = self.stream.inner.as_mut();
            if read_size == 0 {
                // xd3_set_flags
                stream.flags |= binding::xd3_flags::XD3_FLUSH as i32;
                self.eof = true;
            }
            // xd3_avail_input
            stream.next_in = input_buf.as_ptr();
            stream.avail_in = read_size as u32;
        }

        Ok(())
    }

    fn write_output<W>(&mut self, mut output: W) -> io::Result<()>
    where
        W: io::Write,
    {
        let out_data = {
            let stream = self.stream.inner.as_mut();
            unsafe { std::slice::from_raw_parts(stream.next_out, stream.avail_out as usize) }
        };
        output.write_all(out_data)?;

        // xd3_consume_output
        self.stream.inner.as_mut().avail_out = 0;
        Ok(())
    }

    fn step(&mut self, mode: ProcessMode) -> binding::xd3_rvalues {
        unsafe {
            let stream = self.stream.inner.as_mut();
            std::mem::transmute(match mode {
                ProcessMode::Encode => binding::xd3_encode_input(stream),
                ProcessMode::Decode => binding::xd3_decode_input(stream),
            })
        }
    }
}

impl<R> ProcessState<R>
where
    R: AsyncRead + Unpin,
{
    async fn read_input_async<R2>(&mut self, mut input: R2) -> io::Result<()>
    where
        R2: Unpin + AsyncRead,
    {
        let input_buf = &mut self.input_buf;

        let read_size = match input.read(input_buf).await {
            Ok(n) => n,
            Err(_e) => {
                debug!("error on read: {:?}", _e);
                return Err(io::Error::new(io::ErrorKind::Other, "xd3: read_input"));
            }
        };

        {
            let stream = self.stream.inner.as_mut();
            if read_size == 0 {
                // xd3_set_flags
                stream.flags |= binding::xd3_flags::XD3_FLUSH as i32;
                self.eof = true;
            }
            // xd3_avail_input
            stream.next_in = input_buf.as_ptr();
            stream.avail_in = read_size as u32;
        }

        Ok(())
    }

    async fn write_output_async<W>(&mut self, mut output: W) -> io::Result<()>
    where
        W: Unpin + AsyncWrite,
    {
        let out_data = {
            let stream = self.stream.inner.as_mut();
            unsafe { std::slice::from_raw_parts(stream.next_out, stream.avail_out as usize) }
        };
        output.write_all(out_data).await?;

        // xd3_consume_output
        self.stream.inner.as_mut().avail_out = 0;
        Ok(())
    }
}
