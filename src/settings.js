'use strict';
// 设置窗口
const { api, handlePaths, importConfigFile, baseName } = window.CD;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
let S = null, P = { os: 'windows' };

const PRESETS = {
  '数码屏': { fontFamily: 'Bahnschrift', fontWeight: 700, textColor: '#ffffff', bgStyle: 'solid', bgColor: '#0f172a', bgOpacity: 0.78, radius: 18, borderWidth: 0, shadow: true, textShadow: false, glow: false, warnColor: '#ff4d4f' },
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
  fillFonts();
  $('#linkUnsupported').hidden = P.linkSupported !== false;
  if (P.os === 'macos') $('#tipTray').textContent = '隐藏或穿透后,可在屏幕顶部菜单栏图标找回:左键显示/隐藏,右键菜单';
  if (P.os === 'macos') { for (const el of $$('[data-hk]')) el.placeholder = '未设置'; }
}

// 字体只能从下拉里选;旧设置里的自定义字体名也会保留为一项
function fillFonts() {
  const sel = $('#fontSel');
  if (!sel) return;
  const list = [...new Set(['Bahnschrift', ...(FONTS[P.os] || FONTS.windows)])];
  if (S && S.fontFamily && !list.includes(S.fontFamily)) list.push(S.fontFamily);
  sel.innerHTML = '<option value="">系统默认</option>' + list.map((f) => `<option value="${f}" style="font-family:'${f}'">${f}</option>`).join('');
  if (S) sel.value = S.fontFamily || '';
}

// ---------- 侧栏折叠 ----------
function setCollapsed(c) {
  document.body.classList.toggle('collapsed', c);
  const b = $('#btnCollapse');
  b.setAttribute('aria-expanded', String(!c)); b.title = c ? '展开侧栏' : '折叠侧栏';
  try { localStorage.setItem('sideCollapsed', c ? '1' : ''); } catch (e) { /* ignore */ }
}
$('#btnCollapse').onclick = () => setCollapsed(!document.body.classList.contains('collapsed'));
try { if (localStorage.getItem('sideCollapsed')) setCollapsed(true); } catch (e) { /* ignore */ }

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

// ---------- 环节列表编辑器(默认环节和文件专属环节共用) ----------
const totalText = (segs) => { const s = segs.reduce((a, x) => a + x.sec, 0); return s ? '共 ' + window.CDParse.fmtDur(s) : ''; };
function renderSegEditor(box, segs, onChange) {
  box.innerHTML = '';
  const list = segs.length ? segs : [{ name: '', sec: 60 }];
  const emit = () => {
    const rows = [...box.querySelectorAll('.seg')];
    const out = [];
    let bad = false;
    rows.forEach((r) => {
      const sec = window.CDParse.parseDuration(r.querySelector('.sd').value);
      r.classList.toggle('bad', sec == null);
      if (sec == null) { bad = true; return; }
      out.push({ name: r.querySelector('.sn').value.trim(), sec });
    });
    if (!bad && out.length) onChange(out);
  };
  list.forEach((sg, i) => {
    const r = document.createElement('div');
    r.className = 'seg';
    r.innerHTML = '<span class="idx"></span><input class="sn" type="text" placeholder="名称(可留空)" aria-label="环节名称"><input class="sd" type="text" placeholder="时长" aria-label="时长"><button class="btn icon" data-a="up" title="上移" aria-label="上移"><svg><use href="#i-up"/></svg></button><button class="btn icon" data-a="down" title="下移" aria-label="下移"><svg><use href="#i-down"/></svg></button><button class="btn icon" data-a="del" title="删除" aria-label="删除"><svg><use href="#i-x"/></svg></button>';
    r.querySelector('.idx').textContent = i + 1;
    r.querySelector('.sn').value = sg.name || '';
    r.querySelector('.sd').value = window.CDParse.fmtDur(sg.sec);
    r.addEventListener('input', emit);
    r.addEventListener('click', (e) => {
      const a = e.target.closest('button') && e.target.closest('button').dataset.a;
      if (!a) return;
      const cur = [...box.querySelectorAll('.seg')].map((x) => ({ name: x.querySelector('.sn').value.trim(), sec: window.CDParse.parseDuration(x.querySelector('.sd').value) || sg.sec }));
      if (a === 'del') cur.splice(i, 1);
      else if (a === 'up' && i > 0) [cur[i - 1], cur[i]] = [cur[i], cur[i - 1]];
      else if (a === 'down' && i < cur.length - 1) [cur[i + 1], cur[i]] = [cur[i], cur[i + 1]];
      onChange(cur.length ? cur : [{ name: '', sec: 60 }]);
    });
    box.appendChild(r);
  });
  const add = document.createElement('button');
  add.className = 'btn';
  add.innerHTML = '<svg><use href="#i-plus"/></svg><span>添加环节</span>';
  add.onclick = () => {
    const cur = [...box.querySelectorAll('.seg')].map((x) => ({ name: x.querySelector('.sn').value.trim(), sec: window.CDParse.parseDuration(x.querySelector('.sd').value) || 60 }));
    cur.push({ name: '', sec: cur.length ? cur[cur.length - 1].sec : 60 });
    onChange(cur);
  };
  box.appendChild(add);
}

// ---------- 演示文件列表 ----------
let fileState = { missing: [], active: null }, openProfile = null;
async function refreshFileState() { try { fileState = await api.invoke('check_files'); } catch (e) { /* ignore */ } renderFiles(); }

function renderFiles() {
  const box = $('#fileList');
  box.innerHTML = '';
  $('#fileCount').textContent = S.linkFiles.length ? `(${S.linkFiles.length})` : '';
  $('#btnClear').hidden = !S.linkFiles.length;
  if (!S.linkFiles.length) { box.innerHTML = '<div class="row empty">还没有添加演示文件</div>'; return; }
  S.linkFiles.forEach((f) => {
    const [k, label] = kindOf(f);
    const missing = fileState.missing.includes(f), active = fileState.active === f;
    const prof = (S.fileProfiles || {})[f];
    const wrap = document.createElement('div');
    wrap.className = 'filewrap' + (active ? ' active' : '');
    const row = document.createElement('div');
    row.className = 'row file' + (missing ? ' missing' : '');
    row.innerHTML = '<div class="info"><div class="ic"></div><div><div class="t"></div><div class="d"></div></div></div><span class="badge"></span><button class="btn" data-a="prof"></button><button class="btn icon" data-a="rm" title="移除" aria-label="移除"><svg><use href="#i-x"/></svg></button>';
    const ic = row.querySelector('.ic'); ic.textContent = label; ic.classList.add(k);
    row.querySelector('.t').textContent = baseName(f);
    row.querySelector('.d').textContent = missing ? '找不到文件,可能已被移动或删除 · ' + dirOf(f) : dirOf(f);
    row.querySelector('.badge').textContent = active ? '正在放映' : missing ? '找不到' : '';
    row.querySelector('.badge').hidden = !(active || missing);
    const pb = row.querySelector('[data-a=prof]');
    pb.textContent = prof ? '专属计时 · ' + prof.segments.length + ' 个环节' : '使用默认计时';
    pb.classList.toggle('on', !!prof);
    row.title = f;
    row.addEventListener('click', (e) => {
      const a = e.target.closest('button') && e.target.closest('button').dataset.a;
      if (a === 'rm') api.send('remove_link', { path: f });
      if (a === 'prof') { openProfile = openProfile === f ? null : f; renderFiles(); }
    });
    wrap.appendChild(row);
    if (openProfile === f) {
      const ed = document.createElement('div');
      ed.className = 'profile';
      ed.innerHTML = '<div class="d">放映这个文件时自动使用下面的环节(例如题库每页 8 秒,汇报整体 10 分钟)。</div><div class="seglist"></div><div class="btns"><label class="inl"><span class="switch"><input type="checkbox" class="pl"><i></i></span> 循环</label><button class="btn subtle" data-a="clear">改用默认计时</button></div>';
      const cur = prof || { segments: JSON.parse(JSON.stringify(S.segments)), loop: S.loop };
      const save = (segments) => api.set({ fileProfiles: { ...(S.fileProfiles || {}), [f]: { ...cur, segments } } });
      renderSegEditor(ed.querySelector('.seglist'), cur.segments, save);
      const pl = ed.querySelector('.pl'); pl.checked = !!cur.loop;
      pl.onchange = () => api.set({ fileProfiles: { ...(S.fileProfiles || {}), [f]: { ...cur, loop: pl.checked } } });
      ed.querySelector('[data-a=clear]').onclick = () => { const p = { ...(S.fileProfiles || {}) }; delete p[f]; api.set({ fileProfiles: p }); openProfile = null; };
      wrap.appendChild(ed);
    }
    box.appendChild(wrap);
  });
}

function refresh() {
  if (P && S && !$$('#fontSel option').some((o) => o.value === (S.fontFamily || ''))) fillFonts();
  for (const el of $$('[data-k]')) {
    const v = S[el.dataset.k];
    if (document.activeElement !== el) {
      if (el.type === 'checkbox') el.checked = !!v; else el.value = v ?? '';
    }
    fmtOut(el); sliderFill(el);
  }
  for (const el of $$('[data-hk]')) el.value = S.hotkeys[el.dataset.hk] || '';
  if (document.activeElement !== $('#segText')) $('#segText').value = window.CDParse.segmentsToText(S.segments);
  const typing = (box) => document.activeElement && document.activeElement.tagName === 'INPUT' && box.contains(document.activeElement);
  if (!typing($('#segList'))) renderSegEditor($('#segList'), S.segments, (segments) => api.set({ segments }));
  $('#segTotal').textContent = totalText(S.segments);
  renderPreview();
  $('#fontName').textContent = S.fontFile ? baseName(S.fontFile) : '未选择';
  $('#btnFontClear').hidden = !S.fontFile;
  $('#titleColorOn').checked = !!S.titleColor;
  $('#titleColor').value = S.titleColor || S.textColor || '#ffffff';
  $('#titleColor').disabled = !S.titleColor;
  for (const b of $$('#quick button')) b.classList.toggle('on', S.segments.length === 1 && S.segments[0].sec === Number(b.dataset.sec));
  if (!typing($('#fileList'))) renderFiles();
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
  if (b.dataset.armed) { delete b.dataset.armed; b.textContent = '全部移除'; api.send('remove_link', { path: null }); return; }
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

// ---------- 实时预览 ----------
const PV_BG = { dark: '#2b2d31', light: '#e9ecf1', slide: 'linear-gradient(135deg,#f4f4f4,#bcc6d6)' };
let pvBg = 'dark';
function hexA(h, a) {
  h = String(h || '#000').replace('#', '');
  if (h.length === 3) h = [...h].map((c) => c + c).join('');
  const n = parseInt(h, 16) || 0;
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}
function initPreview() {
  for (const id of ['p-look', 'p-title']) {
    const card = document.createElement('div');
    card.className = 'pvcard';
    card.innerHTML = '<div class="pvhead"><span>实时预览</span><span class="seg-ctl" role="group" aria-label="预览背景"><button data-bg="dark" class="on">深色</button><button data-bg="light">浅色</button><button data-bg="slide">幻灯片</button></span></div><div class="pvstage"><div class="pvbox"><div class="pvtitle"></div><div class="pvlabel"></div><div class="pvtime">01:00</div><div class="pvbar"><i></i></div></div></div>';
    $('#' + id).prepend(card);
    card.querySelector('.seg-ctl').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b) { pvBg = b.dataset.bg; renderPreview(); } });
  }
}
function renderPreview() {
  if (!S) return;
  const first = S.segments[0] || { sec: 60, name: '' };
  const scale = 0.62; // 预览按比例缩小
  const fs = Math.min(S.fontSize, 140) * scale;
  const sec = first.sec, p = (n) => String(n).padStart(2, '0');
  const time = S.timeFormat === 'sec' ? String(sec) : sec >= 3600 && S.timeFormat !== 'mm:ss' ? `${Math.floor(sec / 3600)}:${p(Math.floor(sec % 3600 / 60))}:${p(sec % 60)}` : `${p(Math.floor(sec / 60))}:${p(sec % 60)}`;
  for (const card of $$('.pvcard')) {
    card.querySelector('.pvstage').style.background = PV_BG[pvBg];
    $$('.seg-ctl button', card).forEach((b) => b.classList.toggle('on', b.dataset.bg === pvBg));
    const box = card.querySelector('.pvbox'), a = Number(S.bgOpacity);
    box.style.background = S.bgStyle === 'none' ? 'transparent' : S.bgStyle === 'gradient' ? `linear-gradient(135deg, ${hexA(S.bgColor, a)}, ${hexA(S.bgColor2, a)})` : hexA(S.bgColor, a);
    box.style.borderRadius = S.radius * scale + 'px';
    box.style.border = S.borderWidth > 0 ? `${Math.max(1, S.borderWidth * scale)}px solid ${S.borderColor}` : 'none';
    box.style.boxShadow = S.shadow && S.bgStyle !== 'none' ? '0 4px 14px rgba(0,0,0,.35)' : 'none';
    box.style.padding = `${S.padY * scale}px ${S.padX * scale}px`;
    box.style.color = S.textColor;
    box.style.fontWeight = S.fontWeight;
    box.style.fontFamily = S.fontFamily ? `"${S.fontFamily}", ` + (P.os === 'macos' ? '-apple-system, sans-serif' : '"Segoe UI Variable Display", "Segoe UI", sans-serif') : (P.os === 'macos' ? '-apple-system, "SF Pro Display", "PingFang SC", sans-serif' : '"Segoe UI Variable Display", Bahnschrift, "Segoe UI", "Microsoft YaHei UI", sans-serif');
    const sh = [];
    if (S.textShadow) sh.push('0 2px 8px rgba(0,0,0,.65)');
    if (S.glow) sh.push(`0 0 8px ${S.textColor}`, `0 0 20px ${S.textColor}`);
    box.style.textShadow = sh.join(',') || 'none';
    box.style.opacity = Math.min(1, Math.max(0.2, Number(S.windowOpacity) || 1));
    const t = box.querySelector('.pvtitle'), title = (S.titleText || '').trim();
    t.textContent = title; t.style.display = S.showTitle && title ? '' : 'none';
    t.style.fontSize = Math.max(10, fs * (Number(S.titleScale) || 0.34)) + 'px';
    t.style.color = S.titleColor || '';
    const l = box.querySelector('.pvlabel');
    l.textContent = first.name ? first.name + (S.segments.length > 1 ? ` (1/${S.segments.length})` : '') : '';
    l.style.display = S.showLabel && l.textContent ? '' : 'none';
    l.style.fontSize = Math.max(9, fs * 0.28) + 'px';
    const tm = box.querySelector('.pvtime');
    tm.textContent = time; tm.style.fontSize = fs + 'px'; tm.style.letterSpacing = S.letterSpacing * scale + 'px';
    const bar = box.querySelector('.pvbar');
    bar.style.display = S.showProgress ? '' : 'none';
    bar.style.height = Math.max(2, (Number(S.progressHeight) > 0 ? Number(S.progressHeight) : Math.max(3, S.fontSize * 0.06)) * scale) + 'px';
    box.classList.toggle('bar-top', S.progressPosition === 'top');
  }
}

// ---------- 提示条 ----------
api.on('toast', (m) => {
  const el = $('#toast');
  el.textContent = m; el.classList.add('show');
  clearTimeout(el._t); el._t = setTimeout(() => el.classList.remove('show'), 3200);
});

// ---------- 启动 ----------
let lastFiles = '';
api.on('settings', (s) => { S = s; refresh(); const k = JSON.stringify(s.linkFiles); if (k !== lastFiles) { lastFiles = k; refreshFileState(); } });
initPreview();
api.on('link', refreshFileState);
window.addEventListener('focus', refreshFileState);
initPlatform().then(() => api.getSettings()).then((s) => { S = s; lastFiles = JSON.stringify(s.linkFiles); refresh(); refreshFileState(); });
