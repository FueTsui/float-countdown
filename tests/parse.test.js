// 计时配置解析的测试:node --test tests/
const test = require('node:test');
const assert = require('node:assert/strict');
global.window = {};
require('../src/parse.js');
const { parseLine, parseText, parseConfig, parseDuration, fmtDur, segmentsToText } = window.CDParse;

test('各种时长写法', () => {
  assert.deepEqual(parseLine('题库 8秒'), { name: '题库', sec: 8 });
  assert.deepEqual(parseLine('答题 1分30秒'), { name: '答题', sec: 90 });
  assert.deepEqual(parseLine('休息 2:00'), { name: '休息', sec: 120 });
  assert.deepEqual(parseLine('45'), { name: '', sec: 45 });
  assert.deepEqual(parseLine('演讲 1h30m'), { name: '演讲', sec: 5400 });
  assert.deepEqual(parseLine('开场,30'), { name: '开场', sec: 30 });
});

test('忽略空行和注释,无效行返回 null', () => {
  assert.equal(parseLine(''), null);
  assert.equal(parseLine('# 注释'), null);
  assert.equal(parseLine('没有时长'), null);
  assert.equal(parseLine('0秒'), null);
});

test('多行文本', () => {
  assert.equal(parseText('题库 8秒\n\n答题 30秒\n乱写').length, 2);
});

test('parseDuration 只接受纯时长', () => {
  assert.equal(parseDuration('8秒'), 8);
  assert.equal(parseDuration('1分30秒'), 90);
  assert.equal(parseDuration('2:00'), 120);
  assert.equal(parseDuration('45'), 45);
  assert.equal(parseDuration('题库 8秒'), null);
  assert.equal(parseDuration(''), null);
  assert.equal(parseDuration('0'), null);
});

test('JSON 配置', () => {
  const r = parseConfig('{"segments":[{"name":"A","sec":10},{"name":"B","duration":"1分"}],"loop":true}', '.json');
  assert.deepEqual(r.segments, [{ name: 'A', sec: 10 }, { name: 'B', sec: 60 }]);
  assert.equal(r.loop, true);
});

test('格式化往返', () => {
  assert.equal(fmtDur(90), '1分30秒');
  assert.equal(fmtDur(3600), '1小时');
  const segs = [{ name: '题库', sec: 8 }, { name: '答题', sec: 90 }];
  assert.deepEqual(parseText(segmentsToText(segs)), segs);
});
