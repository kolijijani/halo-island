//! GPU genie, ported from MacDock (genie.rs + genie_gpu.rs). The window snapshot is uploaded once as a
//! mipmapped texture; each frame only the 128-row strip mesh is drawn with D3D11 (4x MSAA) into a composition
//! swap chain shown by DirectComposition. The pixel shader cuts the window's rounded corners and draws its
//! soft shadow, so the genie starts and ends looking like the real window.
//!
//! Zero cost at rest: the overlay window, swap chain, MSAA target and texture live for one animation (~0.5 s)
//! and go with `GenieGpu`. It borrows the island's D3D11 device; only the compiled shader (a few KB) stays.

use crate::dock_genie::GenieDir;
use crate::gfx::{Image, SharedD3d};
use crate::window::MonitorInfo;
use halo_core::geom::Rect;
use std::sync::OnceLock;
use std::time::Instant;
use windows::core::{s, w, Interface, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, E_FAIL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dwm::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Rows of the strip mesh: each strip is two affine triangles, so fewer rows show as kinks in the curve.
pub const ROWS: usize = 128;
const FMT: DXGI_FORMAT = DXGI_FORMAT_B8G8R8A8_UNORM;
const SAMPLES: u32 = 4;
const CLASS: PCWSTR = w!("Halo.DockGenieGpu");
/// MacDock's defaults (and the mockup's 34 frames at 60 Hz).
pub const MINIMIZE_SECS: f64 = 0.56;
pub const RESTORE_SECS: f64 = 0.50;
/// After a restore lands: frames for the real window to paint, then the snapshot dissolves into it.
const RESTORE_PAINT: f64 = 0.05;
const RESTORE_FADE: f64 = 0.12;
/// Windows 11's window corner radius and (roughly) its active-window shadow, at 100% scale.
const CORNER: f32 = 8.0;
const SHADOW_SIGMA: f32 = 10.0;
const SHADOW_DROP: f32 = 4.0;
const SHADOW_ALPHA: f32 = 0.32;

const SHADER: &str = r#"
cbuffer C : register(b0) { float2 size; float alpha; float radius; float2 win; float sigma; float shadow; float drop; float3 pad; };
struct V { float2 pos : POSITION; float2 uv : TEXCOORD; };
struct P { float4 pos : SV_POSITION; float2 uv : TEXCOORD; };
P vs(V v) {
    P o;
    o.pos = float4(v.pos.x / size.x * 2.0 - 1.0, 1.0 - v.pos.y / size.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    return o;
}
Texture2D tex : register(t0);
SamplerState smp : register(s0);
float box(float2 p, float2 half_, float r) {
    float2 q = abs(p) - half_ + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
float4 ps(P p) : SV_TARGET {
    float2 px = p.uv * win;
    float2 half_ = win * 0.5;
    float d = box(px - half_, half_, radius);
    float inside = saturate(0.5 - d / max(fwidth(d), 1e-4));
    float3 c = tex.Sample(smp, saturate(p.uv)).rgb;
    float sh = 0.0;
    if (shadow > 0.0) {
        float ds = box(px - half_ - float2(0.0, drop), half_, radius);
        sh = shadow * (0.5 - 0.5 * tanh(0.8 * ds / sigma));
    }
    return float4(c * inside, inside + (1.0 - inside) * sh) * alpha;
}
"#;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl R {
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn grow(&self, m: f64) -> R {
        R { x: self.x - m, y: self.y - m, w: self.w + 2.0 * m, h: self.h + 2.0 * m }
    }
    fn union(&self, o: &R) -> R {
        let (l, t) = (self.x.min(o.x), self.y.min(o.y));
        R { x: l, y: t, w: self.right().max(o.right()) - l, h: self.bottom().max(o.bottom()) - t }
    }
}

/// How a window is dressed, in screen pixels: size, margin drawn around it, corner radius and shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    w: f32,
    h: f32,
    margin: f32,
    radius: f32,
    sigma: f32,
    shadow: f32,
    drop: f32,
}

/// Windows 11 rounds window corners (build 22000 on); Windows 10 does not.
fn rounded_corners() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
        let mut buf = [0u16; 16];
        let mut len = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
                w!("CurrentBuildNumber"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut len),
            )
        };
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        r.is_ok() && String::from_utf16_lossy(&buf[..n]).parse::<u32>().is_ok_and(|b| b >= 22000)
    })
}

/// The look DWM gives a window at `win`: rounded corners on Windows 11 and a soft shadow, scaled to the
/// monitor. A maximized window (it fills the work area) has neither.
fn look_of(win: R, mon: &MonitorInfo) -> Look {
    let s = mon.scale().max(1.0);
    let wk = mon.work;
    let maxed = win.x <= wk.left as f64 + 1.0
        && win.y <= wk.top as f64 + 1.0
        && win.right() >= wk.right as f64 - 1.0
        && win.bottom() >= wk.bottom as f64 - 1.0;
    if maxed {
        return Look { w: win.w as f32, h: win.h as f32, margin: 0.0, radius: 0.0, sigma: 1.0, shadow: 0.0, drop: 0.0 };
    }
    let (sigma, drop) = (SHADOW_SIGMA * s, SHADOW_DROP * s);
    Look {
        w: win.w as f32,
        h: win.h as f32,
        margin: (drop + 3.0 * sigma).ceil(),
        radius: if rounded_corners() { CORNER * s } else { 0.0 },
        sigma,
        shadow: SHADOW_ALPHA,
        drop,
    }
}

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Quintic ease: acceleration also starts and ends at zero, so the slide lands in the icon without a jolt.
fn smoother(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Genie: first the sides bend into a funnel towards the icon (squeeze), then every row slides down and piles
/// up at the icon's bottom edge (slide). p = 0 is the window, p = 1 is fully inside the icon.
/// Returns (left, right, y) per row edge, screen pixels.
pub fn shape(win: R, icon: R, p: f64) -> Vec<(f64, f64, f64)> {
    let squeeze = smooth(p / 0.45);
    let slide = smoother((p - 0.22) / 0.78);
    let travel = icon.bottom() - win.y;
    let funnel = (icon.y - win.y).max(1.0);
    (0..=ROWS)
        .map(|i| {
            let v = i as f64 / ROWS as f64;
            let y = (win.y + win.h * v + travel * slide).min(icon.bottom());
            let f = smooth((y - win.y) / funnel);
            let l = win.x + (icon.x - win.x) * f * squeeze;
            let r = win.right() + (icon.right() - win.right()) * f * squeeze;
            (l, r, y)
        })
        .collect()
}

/// The display refresh a frame presented now shows at, and the refresh period, in seconds on the QPC clock.
/// Animation time taken from it moves in whole refresh steps, so the motion stays even however late in the
/// loop the frame is drawn. None when DWM gives no timing.
fn next_vblank() -> Option<(f64, f64)> {
    unsafe {
        let mut ti = DWM_TIMING_INFO { cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32, ..Default::default() };
        DwmGetCompositionTimingInfo(HWND::default(), &mut ti).ok()?;
        let (mut now, mut freq) = (0i64, 0i64);
        QueryPerformanceCounter(&mut now).ok()?;
        QueryPerformanceFrequency(&mut freq).ok()?;
        let (vb, per) = (ti.qpcVBlank as f64, ti.qpcRefreshPeriod as f64);
        if per <= 0.0 || freq <= 0 || vb <= 0.0 {
            return None;
        }
        let k = ((now as f64 - vb) / per).floor().max(0.0);
        let f = freq as f64;
        Some(((vb + (k + 1.0) * per) / f, per / f))
    }
}

/// What a frame did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Moving,
    /// A restore just reached the full window: show the real one now, the snapshot dissolves over it.
    Landed,
    Done,
}

unsafe extern "system" fn proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(h, msg, w, l),
    }
}

/// Vertex and pixel shader bytecode, compiled once on first use.
fn shaders() -> Option<&'static (Vec<u8>, Vec<u8>)> {
    static CODE: OnceLock<Option<(Vec<u8>, Vec<u8>)>> = OnceLock::new();
    CODE.get_or_init(|| unsafe {
        let vs = crate::glass::compile_hlsl(SHADER, s!("vs"), s!("vs_4_0")).ok()?;
        let ps = crate::glass::compile_hlsl(SHADER, s!("ps"), s!("ps_4_0")).ok()?;
        Some((vs, ps))
    })
    .as_ref()
}

pub struct GenieGpu {
    hwnd: HWND,
    ctx: ID3D11DeviceContext,
    swap: IDXGISwapChain1,
    _dcomp: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    vbuf: ID3D11Buffer,
    cbuf: ID3D11Buffer,
    sampler: ID3D11SamplerState,
    raster: ID3D11RasterizerState,
    blend: ID3D11BlendState,
    msaa: ID3D11Texture2D,
    rtv: ID3D11RenderTargetView,
    srv: ID3D11ShaderResourceView,
    area: R,
    size: (u32, u32),
    win: R,
    icon: R,
    look: Look,
    dir: GenieDir,
    duration: f64,
    start: Option<Instant>,
    vsync: Option<(f64, f64)>,
    landed: Option<Instant>,
    shown: bool,
}

impl Drop for GenieGpu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

impl GenieGpu {
    /// Set up one genie from the window at `win_rc` to the dock icon at `icon` (both screen pixels).
    /// Fails (and the caller falls back to the CPU genie) without a hardware device.
    pub fn new(mon: &MonitorInfo, img: &Image, win_rc: RECT, icon: Rect, dir: GenieDir, shared: SharedD3d) -> Result<Self> {
        let (dev, ctx, warp) = shared;
        if warp || img.width == 0 || img.height == 0 {
            return Err(E_FAIL.into());
        }
        let win = R {
            x: win_rc.left as f64,
            y: win_rc.top as f64,
            w: (win_rc.right - win_rc.left).max(1) as f64,
            h: (win_rc.bottom - win_rc.top).max(1) as f64,
        };
        let icon = R { x: icon.x as f64, y: icon.y as f64, w: (icon.w as f64).max(12.0), h: (icon.h as f64).max(12.0) };
        let look = look_of(win, mon);
        let a = win.grow(look.margin as f64 + 1.0).union(&icon.grow(2.0));
        let (x, y) = (a.x.floor(), a.y.floor());
        let area = R { x, y, w: (a.right().ceil() - x).max(1.0), h: (a.bottom().ceil() - y).max(1.0) };
        let size = (area.w as u32, area.h as u32);
        unsafe {
            static REG: OnceLock<()> = OnceLock::new();
            let hinst = GetModuleHandleW(None)?;
            REG.get_or_init(|| {
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(proc),
                    hInstance: hinst.into(),
                    lpszClassName: CLASS,
                    ..Default::default()
                };
                RegisterClassExW(&wc);
            });
            // layered + transparent = click-through; with no redirection bitmap the layered part costs nothing
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                CLASS,
                w!("Halo Dock Genie"),
                WS_POPUP,
                x as i32,
                y as i32,
                size.0 as i32,
                size.1 as i32,
                None,
                None,
                hinst,
                None,
            )?;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            match Self::build(hwnd, dev, ctx, img, size) {
                Ok(parts) => Ok(Self {
                    hwnd,
                    ctx: parts.0,
                    swap: parts.1,
                    _dcomp: parts.2,
                    _target: parts.3,
                    _visual: parts.4,
                    vs: parts.5,
                    ps: parts.6,
                    layout: parts.7,
                    vbuf: parts.8,
                    cbuf: parts.9,
                    sampler: parts.10,
                    raster: parts.11,
                    blend: parts.12,
                    msaa: parts.13,
                    rtv: parts.14,
                    srv: parts.15,
                    area,
                    size,
                    win,
                    icon,
                    look,
                    dir,
                    duration: if dir == GenieDir::Restore { RESTORE_SECS } else { MINIMIZE_SECS },
                    start: None,
                    vsync: None,
                    landed: None,
                    shown: false,
                }),
                Err(e) => {
                    let _ = DestroyWindow(hwnd);
                    Err(e)
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    unsafe fn build(
        hwnd: HWND,
        dev: ID3D11Device,
        ctx: ID3D11DeviceContext,
        img: &Image,
        (w, h): (u32, u32),
    ) -> Result<(
        ID3D11DeviceContext,
        IDXGISwapChain1,
        IDCompositionDevice,
        IDCompositionTarget,
        IDCompositionVisual,
        ID3D11VertexShader,
        ID3D11PixelShader,
        ID3D11InputLayout,
        ID3D11Buffer,
        ID3D11Buffer,
        ID3D11SamplerState,
        ID3D11RasterizerState,
        ID3D11BlendState,
        ID3D11Texture2D,
        ID3D11RenderTargetView,
        ID3D11ShaderResourceView,
    )> {
        let (vsb, psb) = shaders().ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let dxgi: IDXGIDevice = dev.cast()?;
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: w,
            Height: h,
            Format: FMT,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        let swap = factory.CreateSwapChainForComposition(&dev, &desc, None)?;
        let dcomp = crate::gfx::dcomp_create_device(&dxgi)?;
        let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
        let visual = dcomp.CreateVisual()?;
        visual.SetContent(&swap)?;
        target.SetRoot(&visual)?;
        dcomp.Commit()?;

        let mut vs = None;
        dev.CreateVertexShader(vsb, None, Some(&mut vs))?;
        let mut ps = None;
        dev.CreatePixelShader(psb, None, Some(&mut ps))?;
        let elems = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                Format: DXGI_FORMAT_R32G32_FLOAT,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                ..Default::default()
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                Format: DXGI_FORMAT_R32G32_FLOAT,
                AlignedByteOffset: 8,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                ..Default::default()
            },
        ];
        let mut layout = None;
        dev.CreateInputLayout(&elems, vsb, Some(&mut layout))?;
        let mut vbuf = None;
        dev.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: (ROWS as u32 + 1) * 2 * 16,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut vbuf),
        )?;
        let mut cbuf = None;
        dev.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: 48,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut cbuf),
        )?;
        let mut sampler = None;
        dev.CreateSamplerState(
            &D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MaxLOD: f32::MAX,
                ..Default::default()
            },
            Some(&mut sampler),
        )?;
        let mut raster = None;
        dev.CreateRasterizerState(
            &D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                MultisampleEnable: true.into(),
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            Some(&mut raster),
        )?;
        // premultiplied "over": corners and shadow blend
        let mut bd = D3D11_BLEND_DESC::default();
        bd.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            SrcBlend: D3D11_BLEND_ONE,
            DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D11_BLEND_OP_ADD,
            SrcBlendAlpha: D3D11_BLEND_ONE,
            DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
        };
        let mut blend = None;
        dev.CreateBlendState(&bd, Some(&mut blend))?;

        let mut msaa = None;
        dev.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: FMT,
                SampleDesc: DXGI_SAMPLE_DESC { Count: SAMPLES, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut msaa),
        )?;
        let msaa = msaa.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let mut rtv = None;
        dev.CreateRenderTargetView(&msaa, None, Some(&mut rtv))?;

        // Mipmapped, so the shrink into the icon stays smooth instead of shimmering.
        let mut t = None;
        dev.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: img.width,
                Height: img.height,
                MipLevels: 0,
                ArraySize: 1,
                Format: FMT,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut t),
        )?;
        let t = t.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        ctx.UpdateSubresource(&t, 0, None, img.pixels.as_ptr() as _, img.width * 4, 0);
        let mut srv = None;
        dev.CreateShaderResourceView(&t, None, Some(&mut srv))?;
        let srv = srv.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        ctx.GenerateMips(&srv);

        let missing = || windows::core::Error::from(E_FAIL);
        Ok((
            ctx,
            swap,
            dcomp,
            target,
            visual,
            vs.ok_or_else(missing)?,
            ps.ok_or_else(missing)?,
            layout.ok_or_else(missing)?,
            vbuf.ok_or_else(missing)?,
            cbuf.ok_or_else(missing)?,
            sampler.ok_or_else(missing)?,
            raster.ok_or_else(missing)?,
            blend.ok_or_else(missing)?,
            msaa,
            rtv.ok_or_else(missing)?,
            srv,
        ))
    }

    /// Seconds since the first frame, in whole display refreshes when DWM gives timing.
    fn elapsed(&mut self) -> f64 {
        let Some(start) = self.start else {
            self.start = Some(Instant::now());
            self.vsync = next_vblank().map(|(v, _)| (v, v));
            return 0.0;
        };
        match self.vsync {
            Some((first, last)) => match next_vblank() {
                Some((v, per)) => {
                    // never repeat or reverse a refresh step, even if DWM's clock hiccups
                    let v = if v < last + per * 0.5 { last + per } else { v };
                    self.vsync = Some((first, v));
                    v - first
                }
                None => {
                    self.vsync = None;
                    start.elapsed().as_secs_f64()
                }
            },
            None => start.elapsed().as_secs_f64(),
        }
    }

    /// Advance and draw one frame.
    pub fn frame(&mut self) -> Step {
        if let Some(l) = self.landed {
            let fade = l.elapsed().as_secs_f64() - RESTORE_PAINT;
            if fade >= RESTORE_FADE {
                return Step::Done;
            }
            let alpha = if fade > 0.0 { 1.0 - smooth(fade / RESTORE_FADE) } else { 1.0 };
            let rows = shape(self.win.grow(self.look.margin as f64), self.icon, 0.0);
            return if self.draw(&rows, alpha as f32).is_ok() { Step::Moving } else { Step::Done };
        }
        let t = (self.elapsed() / self.duration).min(1.0);
        let p = if self.dir == GenieDir::Restore { 1.0 - t } else { t };
        let rows = shape(self.win.grow(self.look.margin as f64), self.icon, p);
        if self.draw(&rows, 1.0).is_err() {
            return Step::Done;
        }
        if t < 1.0 {
            Step::Moving
        } else if self.dir == GenieDir::Restore {
            self.landed = Some(Instant::now());
            Step::Landed
        } else {
            Step::Done
        }
    }

    /// Draw the mesh; `rows` are (left, right, y) in screen pixels around the window grown by the margin.
    fn draw(&mut self, rows: &[(f64, f64, f64)], alpha: f32) -> Result<()> {
        let k = self.look;
        let (ox, oy) = (self.area.x, self.area.y);
        unsafe {
            let ctx = &self.ctx;
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.OMSetBlendState(&self.blend, None, 0xffff_ffff);
            ctx.ClearRenderTargetView(&self.rtv, &[0.0, 0.0, 0.0, 0.0]);
            ctx.OMSetRenderTargets(Some(&[Some(self.rtv.clone())]), None::<&ID3D11DepthStencilView>);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: self.size.0 as f32,
                Height: self.size.1 as f32,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            ctx.RSSetState(&self.raster);
            ctx.IASetInputLayout(&self.layout);
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            let (stride, offset) = (16u32, 0u32);
            ctx.IASetVertexBuffers(0, 1, Some(&Some(self.vbuf.clone())), Some(&stride), Some(&offset));
            ctx.VSSetShader(&self.vs, None);
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetShader(&self.ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));

            ctx.Map(&self.cbuf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m))?;
            *(m.pData as *mut [f32; 12]) = [
                self.size.0 as f32,
                self.size.1 as f32,
                alpha.clamp(0.0, 1.0),
                k.radius,
                k.w,
                k.h,
                k.sigma.max(0.01),
                k.shadow,
                k.drop,
                0.0,
                0.0,
                0.0,
            ];
            ctx.Unmap(&self.cbuf, 0);
            // uv runs past 0..1 over the margin
            let (mu, mv) = (k.margin / k.w.max(1.0), k.margin / k.h.max(1.0));
            let n = (rows.len() - 1) as f32;
            ctx.Map(&self.vbuf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut m))?;
            let v = std::slice::from_raw_parts_mut(m.pData as *mut [f32; 4], rows.len() * 2);
            for (i, &(l, r, y)) in rows.iter().enumerate() {
                let (yy, tv) = ((y - oy) as f32, -mv + (1.0 + 2.0 * mv) * i as f32 / n);
                v[i * 2] = [(l - ox) as f32, yy, -mu, tv];
                v[i * 2 + 1] = [(r - ox) as f32, yy, 1.0 + mu, tv];
            }
            ctx.Unmap(&self.vbuf, 0);
            ctx.PSSetShaderResources(0, Some(&[Some(self.srv.clone())]));
            ctx.Draw((rows.len() * 2) as u32, 0);

            let back: ID3D11Texture2D = self.swap.GetBuffer(0)?;
            ctx.ResolveSubresource(&back, 0, &self.msaa, 0, FMT);
            // The device is the island's too: leave nothing bound that its own drawing doesn't set.
            ctx.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            ctx.OMSetBlendState(None::<&ID3D11BlendState>, None, 0xffff_ffff);
            ctx.RSSetState(None::<&ID3D11RasterizerState>);
            ctx.PSSetShaderResources(0, Some(&[None]));
            self.swap.Present(0, DXGI_PRESENT(0)).ok()?;
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_starts_at_window_and_lands_in_icon() {
        let win = R { x: 200.0, y: 100.0, w: 900.0, h: 600.0 };
        let icon = R { x: 920.0, y: 1000.0, w: 44.0, h: 44.0 };
        let start = shape(win, icon, 0.0);
        assert_eq!(start.len(), ROWS + 1);
        assert!(start.iter().all(|&(l, r, _)| (l - win.x).abs() < 1e-9 && (r - win.right()).abs() < 1e-9));
        assert!((start[0].2 - win.y).abs() < 1e-9 && (start[ROWS].2 - win.bottom()).abs() < 1e-9);
        // mid-way the bottom has bent toward the icon more than the top
        let mid = shape(win, icon, 0.4);
        assert!(mid[ROWS].1 - mid[ROWS].0 < mid[0].1 - mid[0].0);
        // the end piles every row at the icon's bottom, inside its width
        let end = shape(win, icon, 1.0);
        assert!(end.iter().all(|&(l, r, y)| (y - icon.bottom()).abs() < 1e-6
            && l >= icon.x - 1e-6
            && r <= icon.right() + 1e-6));
    }
}
