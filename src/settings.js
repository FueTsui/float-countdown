'use strict';
// 设置窗口
const { api, handlePaths, importConfigFile, baseName } = window.CD;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
let S = null, P = { os: 'windows' };

const PRESETS = {
  '数码屏': { fontFamily: '', fontWeight: 700, textColor: '#ffffff', bgStyle: 'solid', bgColor: '#0f172a', bgOpacity: 0.78, radius: 18, borderWidth: 0, shadow: true, textShadow: false, glow: false, warnColor: '#ff4d4f' },
  '极简': { bgStyle: 'none', textColor: '#ffffff', textShadow: true, shadow: false, glow: false, borderWidth: 0, fontWeight: 700, warnColor: '#ff6b6b' },
  '霓虹': { bgStyle: 'solid', bgColor: '#05050f', bgOpacity: 0.72, textColor: '#22d3ee', glow: true, radius: 14, borderWidth: 2, borderColor: '#22d3ee', fontWeight: 700, shadow: false, textShadow: false, warnColor: '#ff3b8d' },
  '浅色卡片': { bgStyle: 'solid', bgColor: '#ffffff', bgOpacity: 0.94, textColor: '#111827', radius: 24, borderWidth: 0, shadow: true, textShadow: false, glow: false, fontWeight: 700, warnColor: '#dc2626' },
  '亚克力': { bgStyle: 'solid', bgColor: '#202020', bgOpacity: 0.62, textColor: '#ffffff', radius: 8, borderWidth: 1, borderColor: '#3a3a3a', shadow: true, textShadow: false, glow: false, fontWeight: 600, warnColor: '#ff99a4' },
  '渐变': { bgStyle: 'gradient', bgColor: '#6366f1', bgColor2: '#ec4899', bgOpacity: 0.92, textColor: '#ffffff', radius: 28, borderWidth: 0, shadow: true, textShadow: false, glow: false, fontWeight: 900, warnColor: '#fde047' },
  '红色大字': { bgStyle: 'none', textColor: '#ff3b30', textShadow: true, shadow: false, glow: false, borderWidth: 0, fontWeight: 900, warnColor: '#ffffff' },
};
const FONTS = {
  windows: ['Segoe UI Variable Display', 'Bahnschrift', 'Consolas', 'Cascadia Mono', 'Microsoft YaHei UI', '微软雅黑', '等线', '黑体', '宋体', '楷体', 'Arial Black', 'Impact', 'Georgia'],
  macos: ['SF Pro Display', 'SF Pro Rounded', 'SF Mono', 'Menlo', 'PingFang SC', 'Helvetica Neue', 'Avenir Next', 'Futura', 'Georgia', 'Courier New'],
};

// ---------- 系统风格:平台、深浅色、强调色 ----------
function hexToRgb(h) { const n = parseInt(String(h).replace('#', ''), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; }
const mix = (a, b, t) => '#' + a.map((v, i) => Math.round(v + (b[i] - v) * t).toString(16).padStart(2, '0')).join('');
function applyTheme() {
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  const html = document.documentElement;
  html.dataset.theme = dark ? 'dark' : 'light';
  if (P.os === 'windows' || P.os === 'linux') {
    // Win11:浅色用 AccentDark1,深色用 AccentLight2(近似)
    const base = hexToRgb(P.accent || '#0078d4');
    const c = dark ? mix(base, [255, 255, 255], 0.45) : mix(base, [0, 0, 0], 0.18);
    html.style.setProperty('--accent', c);
    html.style.setProperty('--accent-text', c);
    html.style.setProperty('--accent-fg', dark ? '#000000' : '#ffffff');
  } else if (P.os === 'macos' && !CSS.supports('color', '-apple-system-control-accent')) {
    // 旧版 WebKit 不认识系统强调色关键字时用系统蓝
    html.style.setProperty('--accent', '#007aff');
    html.style.setProperty('--accent-text', '#007aff');
  }
}
async function initPlatform() {
  try { P = await api.platform(); } catch (e) { /* 浏览器里预览时 */ }
  const html = document.documentElement;
  html.dataset.os = P.os;
  html.classList.toggle('material', !!(P.mica || P.os === 'macos'));
  applyTheme();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  $('#fonts').innerHTML = (FONTS[P.os] || FONTS.windows).map((f) => `<option value="${f}">`).join('');
  $('#linkUnsupported').hidden = P.linkSupported !== false;
  if (P.os === 'macos') $('#tipTray').textContent = '隐藏或穿透后,可在屏幕顶部菜单栏图标找回:左键显示/隐藏,右键菜单';
  if (P.os === 'macos') { for (const el of $$('[data-hk]')) el.placeholder = '未设置'; }
}

// ---------- 页面切换 ----------
function showPage(name) {
  for (const li of $$('#nav li')) li.classList.toggle('on', li.dataset.page === name);
  for (const p of $$('.page')) p.classList.toggle('on', p.id === 'p-' + name);
  $('#pageTitle').textContent = $(`#nav li[data-page="${name}"] span`).textContent;
  $('#pages').scrollTop = 0;
}
$('#nav').addEventListener('click', (e) => { const li = e.target.closest('li'); if (li) showPage(li.dataset.page); });

// ---------- 绑定控件 ----------
function fmtOut(el) {
  const o = el.parentElement.querySelector('output');
  if (!o) return;
  const v = Number(el.value);
  o.textContent = o.dataset.fmt === 'pct' ? Math.round(v * 100) + '%' : o.dataset.fmt === 'px0' ? (v ? v + ' px' : '自动') : String(v);
}
function sliderFill(el) {
  if (el.type !== 'range') return;
  const p = ((Number(el.value) - Number(el.min)) / (Number(el.max) - Number(el.min))) * 100;
  el.style.setProperty('--p', p + '%');
}
const FILE_KIND = { ppt: ['ppt', 'pptx', 'pptm', 'pps', 'ppsx', 'ppsm', 'potx'], pdf: ['pdf'], key: ['key'], wps: ['dps', 'dpt'], odp: ['odp'] };
function kindOf(p) {
  const e = (String(p).match(/\.([^.\\/]+)$/) || ['', ''])[1].toLowerCase();
  for (const [k, list] of Object.entries(FILE_KIND)) if (list.includes(e)) return [k, e.toUpperCase().slice(0, 4)];
  return ['ppt', e.toUpperCase().slice(0, 4)];
}
function dirOf(p) { const i = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/')); return i > 0 ? p.slice(0, i) : ''; }

function renderFiles() {
  const box = $('#fileList');
  box.innerHTML = '';
  $('#fileCount').textContent = S.linkFiles.length ? `(${S.linkFiles.length})` : '';
  $('#btnClear').hidden = !S.linkFiles.length;
  if (!S.linkFiles.length) { box.innerHTML = '<div class="row empty">还没有添加演示文件</div>'; return; }
  S.linkFiles.forEach((f, i) => {
    const [k, label] = kindOf(f);
    const row = document.createElement('div');
    row.className = 'row file';
    row.innerHTML = `<div class="info"><div class="ic ${k}"></div><div><div class="t"></div><div class="d"></div></div></div><button class="btn icon" title="移除"><svg><use href="#i-x"/></svg></button>`;
    row.querySelector('.ic').textContent = label;
    row.querySelector('.t').textContent = baseName(f);
    row.querySelector('.d').textContent = dirOf(f);
    row.title = f;
    row.querySelector('button').onclick = () => api.send('remove_link', { index: i });
    box.appendChild(row);
  });
}

function refresh() {
  for (const el of $$('[data-k]')) {
    const v = S[el.dataset.k];
    if (document.activeElement !== el) {
      if (el.type === 'checkbox') el.checked = !!v; else el.value = v ?? '';
    }
    fmtOut(el); sliderFill(el);
  }
  for (const el of $$('[data-hk]')) el.value = S.hotkeys[el.dataset.hk] || '';
  if (document.activeElement !== $('#segText')) $('#segText').value = window.CDParse.segmentsToText(S.segments);
  $('#fontName').textContent = S.fontFile ? baseName(S.fontFile) : '未选择';
  $('#btnFontClear').hidden = !S.fontFile;
  $('#titleColorOn').checked = !!S.titleColor;
  $('#titleColor').value = S.titleColor || S.textColor || '#ffffff';
  $('#titleColor').disabled = !S.titleColor;
  for (const b of $$('#quick button')) b.classList.toggle('on', S.segments.length === 1 && S.segments[0].sec === Number(b.dataset.sec));
  renderFiles();
}

document.addEventListener('input', (e) => {
  const el = e.target, k = el.dataset && el.dataset.k;
  if (!k) return;
  let v;
  if (el.type === 'checkbox') v = el.checked;
  else if (el.type === 'range' || el.type === 'number' || 'num' in el.dataset) v = Number(el.value);
  else v = el.value;
  S[k] = v;
  fmtOut(el); sliderFill(el);
  api.set({ [k]: v });
});

// 标题颜色:开关关闭时跟随文字颜色
$('#titleColorOn').addEventListener('change', (e) => api.set({ titleColor: e.target.checked ? ($('#titleColor').value || '#ffffff') : '' }));
$('#titleColor').addEventListener('input', (e) => api.set({ titleColor: e.target.value }));

// 预设
for (const name of Object.keys(PRESETS)) {
  const b = document.createElement('button');
  b.textContent = name;
  b.onclick = () => api.set(PRESETS[name]);
  $('#presets').appendChild(b);
}
// 快捷时长
$('#quick').addEventListener('click', (e) => {
  const sec = Number(e.target.dataset && e.target.dataset.sec);
  if (sec) api.set({ segments: [{ name: '', sec }] });
});
// 顶部控制按钮
document.addEventListener('click', (e) => { const b = e.target.closest('[data-cmd]'); if (b) api.send('send_cmd', { c: b.dataset.cmd }); });

// 环节文本
function note(m, el = $('#note')) { el.textContent = m; clearTimeout(el._t); el._t = setTimeout(() => { el.textContent = ''; }, 3500); }
function applyText() {
  const segs = window.CDParse.parseText($('#segText').value);
  if (!segs.length) { note('没有识别到有效的时长,示例: 题库 8秒'); return; }
  api.set({ segments: segs });
  note(`已应用 ${segs.length} 个环节`);
}
$('#applyText').onclick = applyText;
$('#segText').addEventListener('keydown', (e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') applyText(); });

// 文件相关
async function doImport() {
  const p = await api.invoke('pick_config');
  if (p) { const ok = await importConfigFile(p); note(ok ? '已导入: ' + baseName(p) : '导入失败'); }
}
$('#btnImport').onclick = doImport;
api.on('do-import', () => { showPage('timer'); doImport(); });
$('#btnExport').onclick = () => api.invoke('export_config');
$('#btnFont').onclick = () => api.invoke('pick_font');
$('#btnFontClear').onclick = () => api.set({ fontFile: '' });
$('#btnResetStyle').onclick = () => api.send('reset_style');
// 全部移除:再点一次确认(不会删除文件本身)
$('#btnClear').onclick = (e) => {
  const b = e.currentTarget;
  if (b.dataset.armed) { delete b.dataset.armed; b.textContent = '全部移除'; api.send('remove_link', { index: null }); return; }
  b.dataset.armed = '1'; b.textContent = '再点一次确认移除';
  setTimeout(() => { delete b.dataset.armed; b.textContent = '全部移除'; }, 3000);
};

function showResult(r) {
  if (!r) return;
  let m = r.message || '';
  if (r.configs && r.configs.length) m = (m ? m + ';' : '') + `已导入 ${r.configs.length} 个计时配置`;
  note(m, $('#dropNote'));
}
$('#btnLink').onclick = async () => showResult(await api.invoke('pick_files'));
$('#btnFolder').onclick = async () => showResult(await api.invoke('pick_folder'));

// 拖放:整个窗口都能接收,文件夹会自动识别其中的演示文件
api.onDragDrop(async (p) => {
  const dz = $('#dropzone');
  if (p.type === 'enter' || p.type === 'over') { document.body.classList.add('dragging'); dz.classList.add('hot'); }
  else if (p.type === 'leave') { document.body.classList.remove('dragging'); dz.classList.remove('hot'); }
  else if (p.type === 'drop') {
    document.body.classList.remove('dragging'); dz.classList.remove('hot');
    const r = await handlePaths(p.paths);
    if (r && (r.found || r.added || !(r.configs && r.configs.length))) showPage('slides');
    showResult(r);
  }
});

// ---------- 快捷键录入 ----------
const CODE_MAP = { Space: 'Space', Enter: 'Enter', Escape: 'Esc', Tab: 'Tab', Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Insert: 'Insert' };
for (const el of $$('[data-hk]')) {
  el.addEventListener('focus', () => { el.dataset.old = el.value; el.value = ''; el.placeholder = '请按下组合键…'; });
  el.addEventListener('blur', () => { if (!el.value) el.value = el.dataset.old || ''; el.placeholder = ''; });
  el.addEventListener('keydown', (e) => {
    e.preventDefault();
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;
    if (e.key === 'Backspace' || e.key === 'Delete') { el.dataset.old = ''; api.set({ hotkeys: { [el.dataset.hk]: '' } }); el.blur(); return; }
    let key = null;
    if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
    else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
    else if (/^F\d{1,2}$/.test(e.code)) key = e.code;
    else if (/^Numpad\d$/.test(e.code)) key = e.code;
    else key = CODE_MAP[e.code];
    if (!key) return;
    const mods = [];
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    if (e.metaKey) mods.push(P.os === 'macos' ? 'Cmd' : 'Super');
    if (!mods.length && !/^F\d{1,2}$/.test(key)) return; // 单个普通键容易误触,必须带修饰键(F1-F24 除外)
    const acc = [...mods, key].join('+');
    el.value = acc; el.dataset.old = acc;
    api.set({ hotkeys: { [el.dataset.hk]: acc } });
    el.blur();
  });
}

// ---------- 提示条 ----------
api.on('toast', (m) => {
  const el = $('#toast');
  el.textContent = m; el.classList.add('show');
  clearTimeout(el._t); el._t = setTimeout(() => el.classList.remove('show'), 3200);
});

// ---------- 启动 ----------
api.on('settings', (s) => { S = s; refresh(); });
initPlatform().then(() => api.getSettings()).then((s) => { S = s; refresh(); });
