// 解析计时配置:每行 "名称 时长",时长支持 8秒 / 1分30秒 / 2:00 / 1h30m / 纯数字(秒)
(function () {
const UNIT = /(\d+(?:\.\d+)?)\s*(小时|时|hours?|hrs?|h|分钟|分|minutes?|mins?|m|秒钟|秒|seconds?|secs?|s)(?![a-zA-Z])/gi;
const unitSec = (u) => {
  u = u.toLowerCase();
  if (/^(小时|时|h)/.test(u)) return 3600;
  if (/^(分|m)/.test(u)) return 60;
  return 1;
};
const clean = (s) => s.replace(/^[\s:：\-–—=、.]+|[\s:：\-–—=、.]+$/g, '');

function durFromText(s) {
  const m = s.match(/(\d{1,3}):(\d{1,2})(?::(\d{1,2}))?(?!\d)/);
  if (m) {
    const a = +m[1], b = +m[2];
    const sec = m[3] != null ? a * 3600 + b * 60 + (+m[3]) : a * 60 + b;
    return { sec, rest: s.replace(m[0], ' ') };
  }
  let total = 0, found = false;
  const rest = s.replace(UNIT, (_a, n, u) => { found = true; total += parseFloat(n) * unitSec(u); return ' '; });
  return found ? { sec: total, rest } : null;
}

function parseLine(line) {
  line = String(line).replace(/^﻿/, '').trim();
  if (!line || /^(#|\/\/|;)/.test(line)) return null;
  line = line.replace(/^[-*•]\s+/, '');
  const fields = line.split(/[\t|,，;；]/).map((x) => x.trim()).filter(Boolean);
  let sec = null;
  const names = [];
  for (const f of fields) {
    if (sec == null) {
      const d = durFromText(f);
      if (d && d.sec > 0) { sec = d.sec; const r = clean(d.rest); if (r) names.push(r); continue; }
      if (/^\d+(?:\.\d+)?$/.test(f)) { sec = parseFloat(f); continue; }
      let m = f.match(/^(\d+(?:\.\d+)?)\s+(\S.*)$/);
      if (m) { sec = parseFloat(m[1]); names.push(m[2]); continue; }
      m = f.match(/^(.*\S)\s+(\d+(?:\.\d+)?)$/);
      if (m) { sec = parseFloat(m[2]); names.push(m[1]); continue; }
    }
    names.push(f);
  }
  if (sec == null || !(sec > 0)) return null;
  return { name: clean(names.join(' ')), sec: Math.max(1, Math.round(sec)) };
}

function parseText(text) {
  return String(text).split(/\r?\n/).map(parseLine).filter(Boolean);
}

function normSeg(o) {
  if (o == null) return null;
  if (typeof o === 'string') return parseLine(o);
  if (typeof o === 'number') return o > 0 ? { name: '', sec: Math.round(o) } : null;
  if (typeof o === 'object') {
    const name = String(o.name ?? o.title ?? o.label ?? '');
    let v = o.sec ?? o.seconds ?? o.duration ?? o.time;
    let sec = null;
    if (typeof v === 'number') sec = v;
    else if (typeof v === 'string') { const d = durFromText(v); sec = d ? d.sec : parseFloat(v); }
    return sec > 0 ? { name, sec: Math.max(1, Math.round(sec)) } : null;
  }
  return null;
}

function parseConfig(text, ext) {
  text = String(text).replace(/^﻿/, '');
  if (String(ext).toLowerCase() === '.json' || /^\s*[\[{]/.test(text)) {
    try {
      const j = JSON.parse(text);
      const arr = Array.isArray(j) ? j : (j.segments || j.items || []);
      return {
        segments: arr.map(normSeg).filter(Boolean),
        loop: Array.isArray(j) ? undefined : j.loop,
        settings: Array.isArray(j) ? undefined : j.settings,
      };
    } catch (e) { /* 当作文本继续 */ }
  }
  return { segments: parseText(text) };
}

function fmtDur(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
  return (h ? h + '小时' : '') + (m ? m + '分' : '') + (s || (!h && !m) ? s + '秒' : '');
}
const segmentsToText = (segs) => segs.map((s) => `${s.name ? s.name + ' ' : ''}${fmtDur(s.sec)}`).join('\n');

window.CDParse = { parseLine, parseText, parseConfig, fmtDur, segmentsToText };
})();
