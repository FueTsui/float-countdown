'use strict';
// 前端公用:和 Rust 主进程通信、处理拖入/选中的文件。
(function () {
  const T = window.__TAURI__;
  const invoke = (cmd, args) => T.core.invoke(cmd, args);
  const self = T.webviewWindow.getCurrentWebviewWindow();

  const api = {
    invoke,
    send: (cmd, args) => invoke(cmd, args).catch((e) => console.error(cmd, e)),
    getSettings: () => invoke('get_settings'),
    set: (p) => invoke('set_settings', { p }).catch(console.error),
    on: (ev, fn) => self.listen(ev, (e) => fn(e.payload)),
    // 系统拖放事件:enter / over / drop / leave,带文件或文件夹的完整路径
    onDragDrop: (fn) => self.onDragDropEvent((e) => fn(e.payload)),
    fileSrc: (p) => T.core.convertFileSrc(p),
    platform: () => invoke('platform_info'),
  };

  const extOf = (p) => (String(p).match(/\.[^.\\/]+$/) || [''])[0].toLowerCase();
  const baseName = (p) => String(p).split(/[\\/]/).pop();

  // 导入计时配置文件(txt / csv / json / md)
  async function importConfigFile(path) {
    try {
      const text = await invoke('read_text', { path });
      const r = window.CDParse.parseConfig(text, extOf(path));
      if (!r.segments.length) { api.send('send_toast', { msg: '未识别到有效的时长,请检查文件格式' }); return false; }
      const upd = { segments: r.segments };
      if (typeof r.loop === 'boolean') upd.loop = r.loop;
      if (r.settings && typeof r.settings === 'object') {
        const cur = await api.getSettings();
        const skip = new Set(['x', 'y', 'linkFiles', 'hotkeys', 'segments']);
        for (const [k, v] of Object.entries(r.settings)) if (!skip.has(k) && k in cur && typeof v === typeof cur[k]) upd[k] = v;
      }
      await api.set(upd);
      api.send('send_toast', { msg: `已导入 ${r.segments.length} 个环节: ${baseName(path)}` });
      return true;
    } catch (e) {
      api.send('send_toast', { msg: '读取失败: ' + e });
      return false;
    }
  }

  // 处理拖入的路径:文件夹会自动识别里面的演示文件,配置文件会被导入
  async function handlePaths(paths) {
    if (!paths || !paths.length) return null;
    const res = await invoke('add_paths', { paths });
    for (const c of res.configs || []) await importConfigFile(c);
    return res;
  }

  window.CD = { api, importConfigFile, handlePaths, baseName };
})();
