// Real applications installed on this Windows machine and their extracted icons in mockups/dock/icons/
window.REAL_APPS = [
  { slug: "explorer", name: "File Explorer", exe: "C:\\Windows\\explorer.exe", icon: "icons/explorer.png", pinned: true, running: true, windows: ["c:\\work\\Antigravity 2.0\\02.halo-island — File Explorer", "Downloads — File Explorer"] },
  { slug: "antigravity", name: "Antigravity", exe: "C:\\Users\\koliji\\AppData\\Local\\Programs\\antigravity\\Antigravity.exe", icon: "icons/antigravity.png", pinned: true, running: true, windows: ["02.halo-island — DOCK-INVESTIGATION.md — Antigravity"] },
  { slug: "msedge", name: "Microsoft Edge", exe: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", icon: "icons/msedge.png", pinned: true, running: true, windows: ["Halo Island — kolijijani.github.io/halo-island — Microsoft Edge", "Pull Request #42 · crates/halo/src/window.rs — Microsoft Edge"] },
  { slug: "chrome", name: "Google Chrome", exe: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", icon: "icons/chrome.png", pinned: true, running: false, windows: [] },
  { slug: "powershell", name: "Windows PowerShell", exe: "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe", icon: "icons/powershell.png", pinned: true, running: true, windows: ["Windows PowerShell — c:\\work\\Antigravity 2.0\\02.halo-island"] },
  { slug: "notepad", name: "Notepad", exe: "C:\\Windows\\System32\\notepad.exe", icon: "icons/notepad.png", pinned: true, running: false, windows: [] },
  { slug: "calc", name: "Calculator", exe: "C:\\Windows\\System32\\calc.exe", icon: "icons/calc.png", pinned: true, running: false, windows: [] },
  { slug: "taskmgr", name: "Task Manager", exe: "C:\\Windows\\System32\\Taskmgr.exe", icon: "icons/taskmgr.png", pinned: true, running: true, windows: ["Task Manager"] },
  { slug: "halo", name: "Halo Island", exe: "C:\\Users\\koliji\\AppData\\Local\\Programs\\Halo\\halo.exe", icon: "icons/halo.png", pinned: true, running: true, windows: ["Halo Island — Settings"] },
  { slug: "python", name: "Python 3.13", exe: "C:\\Users\\koliji\\AppData\\Local\\Programs\\Python\\Python313\\python.exe", icon: "icons/python.png", pinned: false, running: false, windows: [] },
  { slug: "cmd", name: "Command Prompt", exe: "C:\\Windows\\System32\\cmd.exe", icon: "icons/cmd.png", pinned: false, running: false, windows: [] },
  { slug: "3utools", name: "3uTools", exe: "C:\\Program Files\\3uToolsV3\\3uTools.exe", icon: "icons/3utools.png", pinned: false, running: false, windows: [] },
  { slug: "regedit", name: "Registry Editor", exe: "C:\\Windows\\regedit.exe", icon: "icons/regedit.png", pinned: false, running: false, windows: [] },
  { slug: "control", name: "Control Panel", exe: "C:\\Windows\\System32\\control.exe", icon: "icons/control.png", pinned: false, running: false, windows: [] },
  { slug: "perfmon", name: "Performance Monitor", exe: "C:\\Windows\\System32\\perfmon.exe", icon: "icons/perfmon.png", pinned: false, running: false, windows: [] },
  { slug: "resmon", name: "Resource Monitor", exe: "C:\\Windows\\System32\\resmon.exe", icon: "icons/resmon.png", pinned: false, running: false, windows: [] },
  { slug: "eventvwr", name: "Event Viewer", exe: "C:\\Windows\\System32\\eventvwr.exe", icon: "icons/eventvwr.png", pinned: false, running: false, windows: [] },
  { slug: "mstsc", name: "Remote Desktop Connection", exe: "C:\\Windows\\System32\\mstsc.exe", icon: "icons/mstsc.png", pinned: false, running: false, windows: [] },
  { slug: "msinfo32", name: "System Information", exe: "C:\\Windows\\System32\\msinfo32.exe", icon: "icons/msinfo32.png", pinned: false, running: false, windows: [] },
  { slug: "dxdiag", name: "DirectX Diagnostic Tool", exe: "C:\\Windows\\System32\\dxdiag.exe", icon: "icons/dxdiag.png", pinned: false, running: false, windows: [] },
  { slug: "mmc", name: "Management Console", exe: "C:\\Windows\\System32\\mmc.exe", icon: "icons/mmc.png", pinned: false, running: false, windows: [] },
  { slug: "cleanmgr", name: "Disk Cleanup", exe: "C:\\Windows\\System32\\cleanmgr.exe", icon: "icons/cleanmgr.png", pinned: false, running: false, windows: [] },
  { slug: "dfrgui", name: "Optimize Drives", exe: "C:\\Windows\\System32\\dfrgui.exe", icon: "icons/dfrgui.png", pinned: false, running: false, windows: [] },
  { slug: "charmap", name: "Character Map", exe: "C:\\Windows\\System32\\charmap.exe", icon: "icons/charmap.png", pinned: false, running: false, windows: [] },
  { slug: "magnify", name: "Magnifier", exe: "C:\\Windows\\System32\\Magnify.exe", icon: "icons/magnify.png", pinned: false, running: false, windows: [] },
  { slug: "osk", name: "On-Screen Keyboard", exe: "C:\\Windows\\System32\\osk.exe", icon: "icons/osk.png", pinned: false, running: false, windows: [] },
  { slug: "narrator", name: "Narrator", exe: "C:\\Windows\\System32\\Narrator.exe", icon: "icons/narrator.png", pinned: false, running: false, windows: [] },
  { slug: "wmplayer", name: "Windows Media Player", exe: "C:\\Program Files (x86)\\Windows Media Player\\wmplayer.exe", icon: "icons/wmplayer.png", pinned: false, running: false, windows: [] },
  { slug: "steps", name: "Steps Recorder", exe: "C:\\Windows\\System32\\psr.exe", icon: "icons/steps.png", pinned: false, running: false, windows: [] },
  { slug: "3uviewer", name: "3uViewer", exe: "C:\\Program Files\\3uToolsV3\\3uViewer.exe", icon: "icons/3uviewer.png", pinned: false, running: false, windows: [] }
];

window.DockDesktop = (function () {
  function pad2(n) { return String(n).padStart(2, "0"); }

  function formatRealClock() {
    var d = new Date();
    return pad2(d.getHours()) + ":" + pad2(d.getMinutes());
  }

  function fitViewport() {
    var wrap = document.getElementById("viewport-fit");
    if (!wrap) return;
    var vw = window.innerWidth, vh = window.innerHeight;
    if (vw === 1920 && vh === 1080) {
      wrap.style.transform = "none";
      return;
    }
    var s = Math.min(vw / 1920, vh / 1080);
    var ox = Math.max(0, (vw - 1920 * s) / 2);
    var oy = Math.max(0, (vh - 1080 * s) / 2);
    wrap.style.transform = "translate(" + ox.toFixed(1) + "px," + oy.toFixed(1) + "px) scale(" + s.toFixed(4) + ")";
  }

  function buildBaseShell(opts) {
    var stage = document.getElementById("stage");
    stage.dataset.wall = "dark";
    stage.dataset.zoom = "100";
    stage.dataset.fullscreen = "false";

    var plane = document.getElementById("dpi-plane");

    // 1. Top notch (for A & B; C builds its own unified notch-strip in #halo-notch-wrap)
    if (!opts.customNotch) {
      var nw = document.getElementById("halo-notch-wrap");
      nw.innerHTML =
        '<svg class="notch-ear" viewBox="0 0 12 12"><path d="M0 0 H12 V12 A12 12 0 0 0 0 0 Z" fill="#050608"/></svg>' +
        '<button id="halo-notch" type="button" title="Halo Island resting notch">' +
          '<span class="mascot-eyes"><span></span><span></span></span>' +
          '<span class="notch-clock" id="live-clock">' + formatRealClock() + '</span>' +
        '</button>' +
        '<svg class="notch-ear" viewBox="0 0 12 12"><path d="M0 0 H12 A12 12 0 0 0 0 12 V0 Z" fill="#050608"/></svg>';
      document.getElementById("halo-notch").addEventListener("click", function () {
        toast("Halo Island notch clicked — top activity layer stays independent of bottom dock.");
      });
    }

    // 2. Real open windows on desktop
    var wl = document.getElementById("windows-layer");
    wl.innerHTML =
      '<div class="desk-win active" data-app="antigravity" style="left:520px;top:92px;width:820px;height:500px;z-index:14">' +
        '<div class="win-bar"><img src="icons/antigravity.png" alt=""><span class="win-title">02.halo-island — docs/DOCK-INVESTIGATION.md — Antigravity</span>' +
        '<div class="win-btns"><button type="button" data-win-act="min" title="Minimize">─</button><button type="button" data-win-act="close" title="Close">✕</button></div></div>' +
        '<div class="win-body"><div class="code-mono"># Dock Investigation — Window, Input, and Rendering Layer\n\n1. Island window: WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE\n   crates/halo/src/window.rs:107 — create_island_window()\n   crates/halo/src/window.rs:134 — create_visual_window() (DirectComposition)\n\n2. FrameClock: IDXGIOutput::WaitForVBlank() -> WM_APP_FRAME (window.rs:460, 713)\n   Parked on Condvar::wait at 0% CPU when settled (window.rs:641-643)\n\n3. FullscreenWatcher: SHAppBarMessage(ABM_NEW) + EVENT_SYSTEM_FOREGROUND\n   crates/halo/src/services/fullscreen.rs:142-195</div></div>' +
      '</div>' +
      '<div class="desk-win" data-app="msedge" style="left:860px;top:210px;width:640px;height:430px;z-index:12">' +
        '<div class="win-bar"><img src="icons/msedge.png" alt=""><span class="win-title">Halo Island — kolijijani.github.io/halo-island — Microsoft Edge</span>' +
        '<div class="win-btns"><button type="button" data-win-act="min" title="Minimize">─</button><button type="button" data-win-act="close" title="Close">✕</button></div></div>' +
        '<div class="win-body"><div style="font-size:18px;font-weight:600;margin-bottom:8px">Halo Island for Windows 11</div>' +
        '<p style="opacity:0.82;margin-bottom:14px">A living notch at the top of your display: media, timers, system counters, and quiet notifications with zero CPU at rest.</p>' +
        '<div class="file-row"><span>Installer</span><span>Halo-Island-Setup.exe</span></div>' +
        '<div class="file-row"><span>Renderer</span><span>Direct2D + DirectComposition</span></div>' +
        '<div class="file-row"><span>Idle clock</span><span>Parked (Condvar, 0 Hz at rest)</span></div></div>' +
      '</div>' +
      '<div class="desk-win" data-app="explorer" style="left:110px;top:410px;width:540px;height:320px;z-index:11">' +
        '<div class="win-bar"><img src="icons/explorer.png" alt=""><span class="win-title">c:\\work\\Antigravity 2.0\\02.halo-island — File Explorer</span>' +
        '<div class="win-btns"><button type="button" data-win-act="min" title="Minimize">─</button><button type="button" data-win-act="close" title="Close">✕</button></div></div>' +
        '<div class="win-body">' +
          '<div class="file-row"><span>📁 crates\\halo\\src</span><span>Win32 + Direct2D binary</span></div>' +
          '<div class="file-row"><span>📁 crates\\halo-core\\src</span><span>Platform-agnostic core</span></div>' +
          '<div class="file-row"><span>📄 docs\\DOCK-INVESTIGATION.md</span><span>Phase 1 report</span></div>' +
          '<div class="file-row"><span>📄 docs\\ledger.jsonl</span><span>Append-only job ledger</span></div>' +
        '</div>' +
      '</div>';

    // Window click focus & min/close buttons
    var topZ = 20;
    wl.querySelectorAll(".desk-win").forEach(function (w) {
      w.addEventListener("mousedown", function () {
        focusWindow(w.dataset.app);
      });
      w.querySelectorAll("button[data-win-act]").forEach(function (b) {
        b.addEventListener("click", function (ev) {
          ev.stopPropagation();
          var act = b.dataset.winAct;
          var slug = w.dataset.app;
          if (act === "min") {
            w.classList.add("minimized");
            toast("Minimized " + slug + " — click its dock icon to restore.");
          } else if (act === "close") {
            w.classList.add("minimized");
            if (opts.onCloseWindow) opts.onCloseWindow(slug);
            toast("Closed " + slug + " window.");
          }
        });
      });

      function focusWindow(slug) {
        wl.querySelectorAll(".desk-win").forEach(function (el) { el.classList.remove("active"); });
        w.classList.remove("minimized");
        w.classList.add("active");
        w.style.zIndex = String(++topZ);
        if (opts.onFocusApp) opts.onFocusApp(slug);
      }
    });

    // 3. Fullscreen app overlay
    var fs = document.getElementById("fs-surface");
    fs.innerHTML =
      '<div style="display:flex;justify-content:space-between;align-items:center;opacity:0.7;font-size:13px">' +
        '<span>FULLSCREEN APP ACTIVE (F11 / Borderless D3D11) — Microsoft Edge</span>' +
        '<span>ABN_FULLSCREENAPP + EVENT_SYSTEM_FOREGROUND → dock &amp; notch hidden (SW_HIDE)</span>' +
      '</div>' +
      '<div style="max-width:760px;margin:auto 0">' +
        '<div style="font-size:13px;letter-spacing:0.14em;text-transform:uppercase;color:#6fb6ff;margin-bottom:12px">Independent Flip Preserved</div>' +
        '<div style="font-size:42px;font-weight:600;line-height:1.15;margin-bottom:16px">Both Halo Island and the dock hide completely when a fullscreen window covers this monitor.</div>' +
        '<p style="font-size:16px;line-height:1.6;color:#9fb0c9">No topmost window remains over the game or video surface, and bottom-edge hover reveal is disabled during fullscreen so stray downward mouse flicks in games never pop up the dock.</p>' +
      '</div>' +
      '<div style="font-size:13px;color:#8ea0bc">Click the <b>idle</b> chip in the top-left QA panel to exit fullscreen.</div>';

    // Live clock update on minute boundary
    setInterval(function () {
      var el = document.getElementById("live-clock");
      if (el) el.textContent = formatRealClock();
    }, 10000);

    window.addEventListener("resize", fitViewport);
    fitViewport();

    return {
      focusOrLaunchApp: function (app) {
        var existing = wl.querySelector('.desk-win[data-app="' + app.slug + '"]');
        wl.querySelectorAll(".desk-win").forEach(function (el) { el.classList.remove("active"); });
        if (existing) {
          existing.classList.remove("minimized");
          existing.classList.add("active");
          existing.style.zIndex = String(++topZ);
        } else {
          // Spawn or reuse dynamic app window so clicking ANY of the 30 icons opens a real window
          var dyn = document.getElementById("dyn-desk-win");
          if (!dyn) {
            dyn = document.createElement("div");
            dyn.id = "dyn-desk-win";
            dyn.className = "desk-win";
            dyn.style.cssText = "left:600px;top:150px;width:620px;height:360px;";
            wl.appendChild(dyn);
          }
          var winTitle = (app.windows && app.windows[0]) || (app.name + " — " + app.exe);
          dyn.dataset.app = app.slug;
          dyn.classList.remove("minimized");
          dyn.classList.add("active");
          dyn.style.zIndex = String(++topZ);
          dyn.innerHTML =
            '<div class="win-bar"><img src="' + app.icon + '" alt=""><span class="win-title">' + escapeHtml(winTitle) + '</span>' +
            '<div class="win-btns"><button type="button" id="dyn-min">─</button><button type="button" id="dyn-close">✕</button></div></div>' +
            '<div class="win-body"><div style="display:flex;align-items:center;gap:14px;margin-bottom:14px">' +
            '<img src="' + app.icon + '" alt="" style="width:40px;height:40px">' +
            '<div><div style="font-size:17px;font-weight:600">' + escapeHtml(app.name) + '</div>' +
            '<div style="font-size:12px;opacity:0.72">' + escapeHtml(app.exe) + '</div></div></div>' +
            '<div class="file-row"><span>Process image</span><span>' + escapeHtml(app.exe) + '</span></div>' +
            '<div class="file-row"><span>Activation</span><span>SetForegroundWindow / SW_RESTORE</span></div></div>';
          dyn.querySelector("#dyn-min").onclick = function () { dyn.classList.add("minimized"); };
          dyn.querySelector("#dyn-close").onclick = function () {
            dyn.classList.add("minimized");
            if (opts.onCloseWindow) opts.onCloseWindow(app.slug);
          };
        }
      }
    };
  }

  function toast(msg) {
    var t = document.getElementById("status-toast");
    if (t) t.textContent = msg;
  }

  function escapeHtml(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  function cloneApps(mode) {
    return window.REAL_APPS.map(function (a, idx) {
      var copy = {
        slug: a.slug,
        name: a.name,
        exe: a.exe,
        icon: a.icon,
        pinned: a.pinned,
        running: a.running,
        windows: a.windows.slice()
      };
      if (mode === "many") {
        copy.running = true;
        if (copy.windows.length === 0) {
          copy.windows = [copy.name + " (" + copy.exe.split("\\").pop() + ")"];
        }
      } else if (mode === "empty") {
        copy.pinned = false;
        copy.running = false;
        copy.windows = [];
      }
      return copy;
    }).filter(function (a) {
      return mode === "many" ? true : (mode === "empty" ? false : (a.pinned || a.running));
    });
  }

  function planeMetrics() {
    var stage = document.getElementById("stage");
    var z125 = stage && stage.dataset.zoom === "125";
    return { width: z125 ? 1536 : 1920, height: z125 ? 864 : 1080, scale: z125 ? 1.25 : 1 };
  }

  return {
    formatRealClock: formatRealClock,
    buildBaseShell: buildBaseShell,
    toast: toast,
    escapeHtml: escapeHtml,
    cloneApps: cloneApps,
    planeMetrics: planeMetrics
  };
})();
