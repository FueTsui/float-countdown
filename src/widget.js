'use strict';
// 悬浮窗:计时逻辑与显示
const { api, handlePaths } = window.CD;
let S = null, OS = 'windows';
const $ = (id) => document.getElementById(id);
const stage = $('stage'), box = $('box'), elTitle = $('title'), elLabel = $('label'), elTime = $('time'), elBar = $('bar'), elFill = $('fill');
const T = { state: 'idle', idx: 0, endAt: 0, remain: 0, total: 0, toast: '', toastUntil: 0 };
let segKey = null, fontLoaded = '', maxBox = 0, lastText = null, lastLabel = null, lastWarn = null, lastSec = -1, lastPct = -1, flashT = 0;
const now = () => performance.now();
// 放映联动命中某个文件、且给它设了专属方案时,用专属方案;否则用默认环节
let profile = null;
const segs = () => {
  const l = profile && profile.segments && profile.segments.length ? profile.segments : S.segments;
  return l && l.length ? l : [{ name: '', sec: 60 }];
};
const loopOn = () => (profile && typeof profile.loop === 'boolean' ? profile.loop : S.loop);

// ---------- 样式 ----------
function hexA(h, a) {
  h = String(h || '#000').replace('#', '');
  if (h.length === 3) h = [...h].map((c) => c + c).join('');
  const n = parseInt(h, 16) || 0;
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}
// 未指定字体时使用系统数字字体:Windows 11 用 Segoe UI Variable / Bahnschrift,macOS 用 SF Pro
function fontStack() {
  const sys = OS === 'macos'
    ? '-apple-system, "SF Pro Display", "SF Pro Rounded", "PingFang SC", sans-serif'
    : '"Segoe UI Variable Display", Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif';
  const f = (S.fontFamily || '').trim();
  const base = !f ? sys : f.includes(',') ? f : `"${f}", ${sys}`;
  return (S.fontFile ? 'CDUser, ' : '') + base;
}
function styleText(warn) {
  const col = warn ? S.warnColor : S.textColor;
  box.style.color = col;
  elTitle.style.color = S.titleColor || '';
  const sh = [];
  if (S.textShadow) sh.push('0 2px 8px rgba(0,0,0,.65)', '0 0 2px rgba(0,0,0,.6)');
  if (S.glow) sh.push(`0 0 10px ${col}`, `0 0 26px ${col}`);
  box.style.textShadow = sh.join(',') || 'none';
}
function apply() {
  const key = JSON.stringify(segs());
  if (key !== segKey) { segKey = key; enterSeg(0); setState('idle'); }
  const a = Number(S.bgOpacity);
  box.style.background = S.bgStyle === 'none' ? 'transparent'
    : S.bgStyle === 'gradient' ? `linear-gradient(135deg, ${hexA(S.bgColor, a)}, ${hexA(S.bgColor2, a)})`
      : hexA(S.bgColor, a);
  box.style.borderRadius = S.radius + 'px';
  box.style.border = S.borderWidth > 0 ? `${S.borderWidth}px solid ${S.borderColor}` : 'none';
  box.style.boxShadow = S.shadow && S.bgStyle !== 'none' ? '0 6px 20px rgba(0,0,0,.4)' : 'none';
  box.style.padding = `${S.padY}px ${S.padX}px`;
  box.style.fontFamily = fontStack();
  box.style.fontWeight = S.fontWeight;
  stage.style.opacity = Math.min(1, Math.max(0.1, Number(S.windowOpacity) || 1));
  elTime.style.fontSize = S.fontSize + 'px';
  elTime.style.letterSpacing = S.letterSpacing + 'px';
  elLabel.style.fontSize = Math.max(11, Math.round(S.fontSize * 0.28)) + 'px';
  // 自定义标题
  const title = (S.titleText || '').trim();
  elTitle.textContent = title;
  elTitle.style.display = S.showTitle && title ? '' : 'none';
  elTitle.style.fontSize = Math.max(11, Math.round(S.fontSize * (Number(S.titleScale) || 0.34))) + 'px';
  // 进度条
  const ph = Number(S.progressHeight) > 0 ? Number(S.progressHeight) : Math.max(3, Math.round(S.fontSize * 0.06));
  elBar.style.height = ph + 'px';
  elBar.style.display = S.showProgress ? '' : 'none';
  box.classList.toggle('bar-top', S.progressPosition === 'top');
  lastPct = -1;
  maxBox = 0; box.style.minWidth = '';
  lastWarn = null; lastLabel = null;
  if (S.fontFile !== fontLoaded) {
    fontLoaded = S.fontFile;
    if (S.fontFile) {
      new FontFace('CDUser', `url("${api.fileSrc(S.fontFile)}")`).load()
        .then((f) => { document.fonts.add(f); box.style.fontFamily = fontStack(); })
        .catch(() => toast('字体文件加载失败'));
    }
  }
  render(true);
}

// ---------- 计时 ----------
function setState(st) { if (T.state !== st) { T.state = st; api.send('timer_state', { state: st }); } }
function enterSeg(i) {
  const s = segs();
  T.idx = Math.min(Math.max(i, 0), s.length - 1);
  T.total = T.remain = s[T.idx].sec * 1000;
}
function clearFlash() { clearTimeout(flashT); box.classList.remove('flash'); }
function start() {
  if (T.state === 'running') return;
  if (T.state === 'finished') enterSeg(T.idx < segs().length - 1 ? T.idx + 1 : 0);
  if (T.remain <= 0) enterSeg(T.idx);
  T.endAt = now() + T.remain; setState('running'); clearFlash(); lastSec = -1;
}
function pause() { if (T.state === 'running') { T.remain = Math.max(0, T.endAt - now()); setState('paused'); } }
function toggle() { T.state === 'running' ? pause() : start(); }
function reset() { enterSeg(T.idx); setState('idle'); clearFlash(); }
function goFirst() { enterSeg(0); setState('idle'); clearFlash(); }
function jump(d) {
  const n = segs().length, was = T.state === 'running';
  enterSeg((T.idx + d + n) % n);
  clearFlash();
  if (was) T.endAt = now() + T.remain; else setState('idle');
}
function finish() {
  T.remain = 0; T.finishedAt = now(); setState('finished');
  if (S.flashOnEnd) { box.classList.add('flash'); flashT = setTimeout(clearFlash, 5200); }
  endBeep();
  if (S.autoPage) {
    setTimeout(() => {
      api.send('next_slide');
      if (S.pageTurn !== 'restart') setTimeout(() => { goFirst(); start(); }, 400);
    }, S.autoPageDelay || 0);
  }
}
function tick() {
  if (!S) return;
  if (T.state === 'running') {
    let rem = T.endAt - now();
    const list = segs();
    while (rem <= 0) {
      if (S.autoNext && T.idx < list.length - 1) { enterSeg(T.idx + 1); T.endAt += T.total; rem = T.endAt - now(); lastSec = -1; continue; }
      if (S.autoNext && loopOn()) { enterSeg(0); T.endAt += T.total; rem = T.endAt - now(); lastSec = -1; continue; }
      if (!S.autoNext && loopOn() && list.length === 1) { enterSeg(0); T.endAt += T.total; rem = T.endAt - now(); continue; }
      finish(); break;
    }
    if (T.state === 'running' && S.tickBeep) {
      const sec = Math.ceil(rem / 1000);
      if (sec !== lastSec) { lastSec = sec; if (sec > 0 && sec <= 3) beep(700, 0.07, S.beepVolume); }
    }
  }
  render(false);
}

// ---------- 显示 ----------
function fmt(ms) {
  ms = Math.max(0, ms);
  const tf = S.timeFormat;
  if (S.decimals && ms < 10000 && (tf === 'auto' || tf === 'sec')) return (Math.ceil(ms / 100) / 10).toFixed(1);
  const s = Math.ceil(ms / 1000), p = (n) => String(n).padStart(2, '0');
  if (tf === 'sec') return String(s);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  if (tf === 'hh:mm:ss' || (tf === 'auto' && h > 0)) return `${h}:${p(m)}:${p(s % 60)}`;
  return `${p(Math.floor(s / 60))}:${p(s % 60)}`;
}
function fmtUp(ms) {
  const s = Math.floor(Math.max(0, ms) / 1000), p = (n) => String(n).padStart(2, '0');
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${p(m)}:${p(s % 60)}` : `${p(m)}:${p(s % 60)}`;
}
function render(force) {
  const t = now(), list = segs();
  const rem = T.state === 'running' ? T.endAt - t : T.remain;
  const fin = T.state === 'finished';
  // 到时后:开启“显示超时”则继续往上数(-00:12),否则显示结束文字
  const text = fin ? (S.overtime ? '-' + fmtUp(t - T.finishedAt) : (S.endText || fmt(0))) : fmt(rem);
  if (text !== lastText || force) { elTime.textContent = text; lastText = text; }
  const toasting = t < T.toastUntil;
  let lab = '';
  if (toasting) lab = T.toast;
  else if (S.showLabel) { const nm = list[T.idx].name || ''; lab = list.length > 1 ? `${nm ? nm + ' ' : ''}(${T.idx + 1}/${list.length})` : nm; }
  if (lab !== lastLabel || force) { elLabel.textContent = lab; elLabel.style.display = lab ? '' : 'none'; lastLabel = lab; }
  const warn = S.warnEnabled && (fin || ((T.state === 'running' || T.state === 'paused') && rem <= Math.min(S.warnSec * 1000, T.total / 2)));
  if (warn !== lastWarn) { lastWarn = warn; styleText(warn); }
  const pct = fin ? 0 : Math.max(0, Math.min(1, rem / T.total));
  if (S.showProgress && Math.abs(pct - lastPct) > 0.002) { lastPct = pct; elFill.style.width = (pct * 100).toFixed(2) + '%'; }
}
function toast(msg) { T.toast = msg; T.toastUntil = now() + 2800; if (S) render(false); }

// ---------- 声音 ----------
let ac = null;
function beep(freq, dur, vol, when = 0) {
  try {
    ac = ac || new AudioContext();
    if (ac.state === 'suspended') ac.resume();
    const o = ac.createOscillator(), g = ac.createGain(), t0 = ac.currentTime + when;
    o.type = 'sine'; o.frequency.value = freq;
    g.gain.setValueAtTime(Math.max(0.001, vol), t0);
    g.gain.exponentialRampToValueAtTime(0.001, t0 + dur);
    o.connect(g); g.connect(ac.destination); o.start(t0); o.stop(t0 + dur + 0.03);
  } catch (e) { /* ignore */ }
}
function endBeep() { if (S.beep) [0, 0.25, 0.5].forEach((w) => beep(988, 0.18, S.beepVolume, w)); }

// ---------- 交互 ----------
let dragging = false, raf = 0;
stage.addEventListener('pointerdown', (e) => {
  if (e.button !== 0 || S.locked) return;
  dragging = true; stage.setPointerCapture(e.pointerId); api.send('drag_start');
});
stage.addEventListener('pointermove', () => {
  if (!dragging || raf) return;
  raf = requestAnimationFrame(() => { raf = 0; api.send('drag_move'); });
});
const endDrag = () => { if (dragging) { dragging = false; api.send('drag_end'); } };
stage.addEventListener('pointerup', endDrag);
stage.addEventListener('pointercancel', endDrag);
stage.addEventListener('dblclick', toggle);
window.addEventListener('contextmenu', (e) => { e.preventDefault(); api.send('show_menu'); });
// Ctrl+滚轮(Mac 为 Cmd+滚轮)调整大小;普通滚动不处理,避免误改。锁定位置时也不调整
window.addEventListener('wheel', (e) => {
  if (!(e.ctrlKey || e.metaKey) || S.locked) return;
  e.preventDefault();
  const v = Math.min(400, Math.max(16, Math.round(S.fontSize + (e.deltaY < 0 ? 4 : -4))));
  S.fontSize = v; api.set({ fontSize: v });
}, { passive: false });

// 拖入文件或文件夹:演示文件直接绑定,文件夹自动识别其中的演示文件,txt/json 等作为计时配置导入
api.onDragDrop((p) => {
  if (p.type === 'enter' || p.type === 'over') box.classList.add('drop');
  else if (p.type === 'leave') box.classList.remove('drop');
  else if (p.type === 'drop') { box.classList.remove('drop'); handlePaths(p.paths); }
});

// 窗口大小随内容变化(宽度只增不减,避免数字跳动时抖动)
new ResizeObserver(() => {
  const bw = box.offsetWidth;
  if (bw > maxBox) { maxBox = bw; box.style.minWidth = bw + 'px'; }
  api.send('resize', { w: stage.offsetWidth, h: stage.offsetHeight });
}).observe(stage);

// ---------- 来自主进程 ----------
const CMD = { toggle, start, pause, reset, next: () => jump(1), prev: () => jump(-1), first: goFirst, pageturn: () => { goFirst(); start(); }, pageprev: () => { goFirst(); start(); } };
api.on('cmd', (c) => { if (S && CMD[c]) CMD[c](); });
api.on('toast', toast);
api.on('settings', (s) => { S = s; apply(); });
api.on('link', (p) => {
  const a = !!(p && p.active), file = p && p.file;
  if (!S) return;
  // 切换到该文件专属的计时方案(没有就用默认)
  profile = a && file && S.fileProfiles && S.fileProfiles[file] ? S.fileProfiles[file] : null;
  if (a && file) toast('联动: ' + file.split(/[\\/]/).pop());
  apply();
  if (!S.linkAutoStart) return;
  if (a) { goFirst(); start(); } else { goFirst(); }
});
Promise.all([api.getSettings(), api.platform().catch(() => ({ os: 'windows' }))]).then(([s, p]) => {
  OS = p.os; S = s; apply(); setInterval(tick, 50);
});
// 供自测脚本读取状态
window.__T = T;
