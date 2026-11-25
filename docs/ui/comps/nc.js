// nicecooling comps: theme switch (?theme=dark|light), icon sprite, app shell, Loop diagram and Graph canvas.
// It only keeps the comps short. It is not part of the product.
(function () {
  var q = new URLSearchParams(location.search).get('theme');
  if (q) document.documentElement.dataset.theme = q;

  var S = { // stroke icons, 16 px grid
    off: '<circle cx="8" cy="8" r="6" stroke-dasharray="3 2.2"/><path d="M4.5 11.5l7-7"/>',
    lock: '<rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5.2 7V5a2.8 2.8 0 0 1 5.6 0v2"/>',
    flask: '<path d="M6 2h4M7 2v4l-4 7a1 1 0 0 0 .9 1.5h8.2A1 1 0 0 0 13 13L9 6V2"/>',
    check: '<path d="M3 8.5l3.2 3L13 4.5"/>',
    shield: '<path d="M8 1.5l5.5 2v4c0 3.5-2.3 6-5.5 7-3.2-1-5.5-3.5-5.5-7v-4z"/>',
    shieldok: '<path d="M8 1.5l5.5 2v4c0 3.5-2.3 6-5.5 7-3.2-1-5.5-3.5-5.5-7v-4z"/><path d="M5.6 8l1.9 1.9 3-3.4"/>',
    eye: '<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>',
    code: '<path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5"/>',
    down: '<path d="M4 6l4 4 4-4"/>', right: '<path d="M6 4l4 4-4 4"/>', left: '<path d="M10 4L6 8l4 4"/>',
    plus: '<path d="M8 3v10M3 8h10"/>', x: '<path d="M4 4l8 8M12 4l-8 8"/>',
    search: '<circle cx="7" cy="7" r="4"/><path d="M10 10l3.5 3.5"/>',
    pencil: '<path d="M3 13l.6-2.8L11 2.8 13.2 5l-7.4 7.4z"/>',
    copy: '<rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M3 10.5V3h7.5"/>',
    trash: '<path d="M3 4.5h10M6 4.5V3h4v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/>',
    info: '<circle cx="8" cy="8" r="6"/><path d="M8 7.5V11M8 5v.2"/>',
    clock: '<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/>',
    app: '<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M2 6h12"/>',
    power: '<path d="M8 2v6M4.6 4.6a5 5 0 1 0 6.8 0"/>',
    moon: '<path d="M13 9.5A5.5 5.5 0 0 1 6.5 3 5.5 5.5 0 1 0 13 9.5z"/>',
    gauge: '<path d="M2.5 11.5a5.5 5.5 0 1 1 11 0M8 11.5l2.6-3.6"/>',
    home: '<path d="M2.5 7.5L8 3l5.5 4.5V13h-11z"/>',
    loop: '<rect x="2" y="4.5" width="12" height="7" rx="3.5"/><path d="M7 3l2 1.5L7 6"/>',
    fan: '<circle cx="8" cy="8" r="1.5"/><path d="M8 6.5V2.5a2.2 2.2 0 0 1 2.2 3.2zM9.3 8.8l3.5 2a2.2 2.2 0 0 1-3.9.4zM6.7 8.8l-3.5 2a2.2 2.2 0 0 0 3.9.4z"/>',
    layers: '<path d="M8 2.5l6 3-6 3-6-3zM2 8.5l6 3 6-3M2 11.5l6 3 6-3"/>',
    bell: '<path d="M4 11V7.5a4 4 0 0 1 8 0V11l1.2 1.5H2.8zM6.8 14.2h2.4"/>',
    chart: '<path d="M2.5 13.5h11M4 11l3-4 2.5 2.5L13 4.5"/>',
    sliders: '<path d="M2.5 5h5.5M11 5h2.5M2.5 11H5M8 11h5.5"/><circle cx="9.5" cy="5" r="1.5"/><circle cx="6.5" cy="11" r="1.5"/>',
    grip: '<path d="M6 4v.1M10 4v.1M6 8v.1M10 8v.1M6 12v.1M10 12v.1" stroke-width="2"/>',
    menu: '<path d="M3 4.5h10M3 8h10M3 11.5h10"/>',
    undo: '<path d="M5 3L2.5 5.5 5 8M2.5 5.5h7a3.5 3.5 0 0 1 0 7H6"/>',
    graph: '<circle cx="3.5" cy="8" r="1.8"/><circle cx="12.5" cy="4" r="1.8"/><circle cx="12.5" cy="12" r="1.8"/><path d="M5.3 8C8 8 8 4 10.7 4M5.3 8C8 8 8 12 10.7 12"/>'
  };
  var F = { // filled icons
    alarm: '<path fill-rule="evenodd" d="M5 1h6l4 4v6l-4 4H5l-4-4V5zM7 4v5h2V4zM7 10.5v2h2v-2z"/>',
    warn: '<path fill-rule="evenodd" d="M8 1l7.4 13.5H.6zM7 6v4.5h2V6zM7 11.5v1.8h2v-1.8z"/>',
    play: '<path d="M5 3.5l7 4.5-7 4.5z"/>'
  };
  var sprite = '<svg style="position:absolute;width:0;height:0">';
  for (var k in S) sprite += '<symbol id="i-' + k + '" viewBox="0 0 16 16">' + S[k] + '</symbol>';
  for (k in F) sprite += '<symbol id="i-' + k + '" viewBox="0 0 16 16">' + F[k] + '</symbol>';
  sprite += '</svg>';
  function icon(n, cls) { return '<svg class="i' + (F[n] ? ' f' : '') + (cls ? ' ' + cls : '') + '"><use href="#i-' + n + '"/></svg>'; }

  function shell() {
    var b = document.body, d = b.dataset;
    if (!d.page) return;
    var kids = Array.prototype.slice.call(b.childNodes);
    function nav(id, label, ic, end) { return '<a class="' + (d.page === id ? 'sel' : '') + '">' + icon(ic) + '<span class="lbl">' + label + '</span>' + (end ? '<span class="end">' + end + '</span>' : '') + '</a>'; }
    var lsEnd = d.ls === 'alarm' ? icon('alarm', 't-alarm') : d.ls === 'warn' ? icon('warn', 't-warn') : d.ls === 'off' ? icon('off', 't-off') : '';
    var al = +d.alarms || 0, wn = +d.warnings || 0;
    var side = '<nav class="sidebar"><div class="app">nicecooling</div>' +
      nav('overview', 'Overview', 'home') + nav('loops', 'Loops', 'loop') + nav('channels', 'Channels', 'fan') + nav('sensors', 'Sensors', 'gauge') +
      '<h6>Devices</h6>' + nav('leakshield', 'Leakshield', 'shield', lsEnd) +
            '<h6>Control</h6>' + nav('profiles', 'Profiles', 'layers') + nav('graphs', 'Graphs', 'graph') + nav('alarms', 'Alarms', 'bell', al + wn ? '<b>' + (al + wn) + '</b>' : '') + nav('history', 'History', 'chart') +
      '<div class="grow"></div>' + nav('settings', 'Settings', 'sliders') + '</nav>';
    var sbAl = al ? '<span class="t-alarm"><b>' + icon('alarm') + ' ' + (d.alarmtext || al + ' Alarm') + '</b></span>' : wn ? '<span class="t-warn"><b>' + icon('warn') + ' ' + wn + ' Warning</b></span>' : '<span>No active Alarms</span>';
    var sb = '<footer class="sb">' + (d.daemon === 'down' ? '<span class="t-alarm"><b>' + icon('alarm') + ' Daemon down</b></span>' : d.daemon === 'off' ? '<span class="t-off">' + icon('off') + ' Daemon offline</span>' : '<span><i class="dot"></i>Daemon connected</span>') +
      '<span>Profile: ' + (d.profile || 'Quiet (rule)') + '</span>' + sbAl + '</footer>';
    var w = document.createElement('div'); w.className = 'win';
    w.innerHTML = side + '<header class="hb">' + '<span class="title">' + d.title + (d.subtitle ? '<small>' + d.subtitle + '</small>' : '') + '</span><span class="right"><span class="btn sm flat icon">' + icon('menu') + '</span><span class="close">' + icon('x') + '</span></span></header><div class="content"></div>' + sb;
    var c = w.querySelector('.content');
    kids.forEach(function (n) { if (!(n.classList && n.classList.contains('scrim')) && n.tagName !== 'SCRIPT') c.appendChild(n); });
    b.insertBefore(w, b.firstChild);
  }

  // Loop diagram. data-loop: {ls, level, pt, st, ft, tt, names:[pump, flow, temp], none}
  // Each of pt/st/ft/tt is [value, caption, state] with state ok|warn|alarm|off.
  function bubble(x, y, tag, d, tx) {
    var st = d[2] || 'ok', ic = st === 'alarm' || st === 'warn' || st === 'flag'; // flag: a note on the value, not a Warning
    return '<g class="' + (st === 'flag' ? 'ok' : st) + '"><circle class="bub" cx="' + x + '" cy="' + y + '" r="18"/><text class="tg" x="' + x + '" y="' + (y + 4) + '">' + tag + '</text>' +
      '<text class="v" x="' + tx + '" y="' + (y - 3) + '">' + d[0] + '</text>' +
      (st === 'flag' ? '<use href="#i-info" x="' + tx + '" y="' + (y + 5) + '" width="12" height="12" fill="none" stroke="var(--fg-dim)" stroke-width="1.5"/>' :
        ic ? '<use href="#i-' + st + '" x="' + tx + '" y="' + (y + 5) + '" width="12" height="12" fill="var(--' + (st === 'alarm' ? 'alarm' : 'warning') + ')"/>' : '') +
      '<text class="c" x="' + (tx + (ic ? 16 : 0)) + '" y="' + (y + 15) + '">' + d[1] + '</text></g>';
  }
  function loop(svg) {
    var L = JSON.parse(svg.dataset.loop), W = Math.round(svg.getBoundingClientRect().width), H = 168, s = '';
    var xs = [Math.round(W * .34), Math.round(W * .58), Math.round(W * .80)], x0 = L.ls ? 88 : 34;
    svg.setAttribute('viewBox', '0 0 ' + W + ' ' + H);
    s += '<path class="w" d="M' + x0 + ' 100H' + (W - 34) + 'a10 10 0 0 1 10 10v30a10 10 0 0 1-10 10H' + x0 + (L.ls ? '' : 'a10 10 0 0 1-10-10v-30a10 10 0 0 1 10-10') + '"/>';
    var ax = Math.round((xs[0] + xs[1]) / 2);
    s += '<path class="w" d="M' + (ax - 6) + ' 93l12 7-12 7M' + (xs[0] - 40) + ' 143l-12 7 12 7"/>';
    if (L.ls) {
      s += '<path class="a" d="M56 60V82"/><rect class="p" x="24" y="82" width="64" height="78" rx="4"/>';
      if (L.level != null) { var h = Math.round(76 * L.level / 100); s += '<rect class="lvl" x="25" y="' + (159 - h) + '" width="62" height="' + h + '"/><text class="nm" x="56" y="122" text-anchor="middle">' + L.level + ' %</text><text x="56" y="140" text-anchor="middle">' + (L.levelLabel || 'Fill level') + '</text>'; }
      s += '<rect class="p" x="8" y="32" width="96" height="28" rx="14"/><text class="nm" x="56" y="50" text-anchor="middle">Leakshield</text>';
      s += '<path class="ld" d="M104 46h10"/>' + bubble(132, 46, 'PT', L.pt, 158);
    } else {
      s += '<text class="nm" x="24" y="43" style="fill:var(--fg-dim)">No Leakshield</text><text x="24" y="61">' + (L.none || 'Pressure is not measured.') + '</text>';
    }
    var bw = Math.min(230, W * .3), bx = Math.round(W * .69 - bw / 2);
    s += '<rect x="' + bx + '" y="138" width="' + Math.round(bw) + '" height="24" rx="4" fill="var(--card)"/><rect class="dash" x="' + bx + '" y="138" width="' + Math.round(bw) + '" height="24" rx="4"/><text x="' + Math.round(W * .69) + '" y="154" text-anchor="middle">Blocks and radiators</text>';
    if (L.st) s += '<path class="ld" d="M' + xs[0] + ' 64V86"/><circle class="p" cx="' + xs[0] + '" cy="100" r="14"/><path class="ink" d="M' + (xs[0] - 5) + ' 93l12 7-12 7z"/>' + bubble(xs[0], 46, 'ST', L.st, xs[0] + 26);
    if (L.ft) s += '<path class="ld" d="M' + xs[1] + ' 64V89"/><rect class="p" x="' + (xs[1] - 18) + '" y="89" width="36" height="22" rx="3"/>' + bubble(xs[1], 46, 'FT', L.ft, xs[1] + 26);
    if (L.tt) s += '<path class="ld" d="M' + xs[2] + ' 64V96"/><circle class="ink" cx="' + xs[2] + '" cy="100" r="4.5"/>' + bubble(xs[2], 46, 'TT', L.tt, xs[2] + 26);
    (L.names || []).forEach(function (n, i) { if (n) s += '<text x="' + xs[i] + '" y="128" text-anchor="middle">' + n + '</text>'; });
    svg.innerHTML = s;
  }

  // Graph canvas. data-flow: {nodes:[{id, x, y, k, par, s, mono, lb, v, u, pins, out, sel, draft, test, warn, st, band}], edges:[[from, to, pin]]}
  // k kind, par main parameter, s subject, lb label, v value, u unit, st state caption, band [scale low, scale high, minimum, maximum, value].
  // The canvas opens with the whole Graph in the view, at 100% at most: the zoom goes to every [data-zoom] element.
  function flow(view) {
    var G = JSON.parse(view.dataset.flow), h = '';
    function pc(a, b, x) { return Math.round((x - a) / (b - a) * 1000) / 10; }
    G.nodes.forEach(function (n) {
      var on = G.edges.some(function (e) { return e[0] === n.id; }), b = n.band;
      h += '<div class="node' + (n.sel ? ' sel' : '') + (n.draft ? ' draft' : '') + '" data-id="' + n.id + '" style="left:' + n.x + 'px;top:' + n.y + 'px">' +
        '<div class="t">' + (n.warn ? icon('warn', 't-warn') : '') + '<b>' + n.k + '</b>' + (n.par ? '<span>' + n.par + '</span>' : '') + (n.test ? '<em class="tag accent">Test</em>' : '') + '</div>' +
        (n.s ? '<div class="s' + (n.mono ? ' mono' : '') + '">' + n.s + '</div>' : '') + (n.warn ? '<div class="s t-warn">' + n.warn + '</div>' : '') +
        '<div class="v">' + (n.lb ? '<span class="lb mono">' + n.lb + '</span>' : '') + '<b>' + n.v + '</b>' + (n.u ? '<span class="u">' + n.u + '</span>' : '') +
        (n.out === false ? '' : '<i class="pin out' + (on ? ' on' : '') + '"></i>') + '</div>' +
        (b ? '<div class="g"><i class="tr"></i><i class="bd" style="left:' + pc(b[0], b[1], b[2]) + '%;right:' + (100 - pc(b[0], b[1], b[3])) + '%"></i>' + (b[4] == null ? '' : '<i class="mk" style="left:' + pc(b[0], b[1], b[4]) + '%"></i>') + '</div>' : '') +
        (n.st ? '<div class="s dim">' + n.st + '</div>' : '') +
        (n.pins || []).map(function (p, i) {
          var c = G.edges.some(function (e) { return e[1] === n.id && e[2] === i; });
          return '<div class="p"><i class="pin' + (c ? ' on' : '') + '" data-pin="' + i + '"></i>' + p + '</div>';
        }).join('') + '</div>';
    });
    view.innerHTML = '<svg class="dots"><defs><pattern id="dots" patternUnits="userSpaceOnUse"><circle r="1.2"/></pattern></defs><rect width="100%" height="100%" fill="url(#dots)"/></svg><div class="flow"><svg class="edges"></svg>' + h + '</div>' + view.innerHTML;
    var f = view.querySelector('.flow'), o = f.getBoundingClientRect(), W = 0, H = 0, s = '';
    function at(el) { var r = el.getBoundingClientRect(); return [r.left + r.width / 2 - o.left, r.top + r.height / 2 - o.top]; }
    f.querySelectorAll('.node').forEach(function (n) { W = Math.max(W, n.offsetLeft + n.offsetWidth); H = Math.max(H, n.offsetTop + n.offsetHeight); });
    G.edges.forEach(function (e) {
      var a = at(f.querySelector('[data-id="' + e[0] + '"] .pin.out')), b = at(f.querySelector('[data-id="' + e[1] + '"] [data-pin="' + e[2] + '"]')), d = Math.max(16, (b[0] - a[0]) / 2);
      s += '<path' + (e[3] ? ' class="sel"' : '') + ' d="M' + a[0] + ' ' + a[1] + 'C' + (a[0] + d) + ' ' + a[1] + ' ' + (b[0] - d) + ' ' + b[1] + ' ' + (b[0] - 11) + ' ' + b[1] + '"/><path class="ar' + (e[3] ? ' sel' : '') + '" d="M' + (b[0] - 5) + ' ' + b[1] + 'l-7-4v8z"/>';
    });
    f.querySelector('.edges').innerHTML = s;
    W += 12; H += 12;
    var z = Math.floor(Math.min(1, view.clientWidth / W, view.clientHeight / H) * 100) / 100;
    var dx = Math.round((view.clientWidth - W * z) / 2), dy = Math.round((view.clientHeight - H * z) / 2), pt = view.querySelector('pattern');
    f.style.transform = 'translate(' + dx + 'px,' + dy + 'px) scale(' + z + ')';
    pt.setAttribute('width', 24 * z); pt.setAttribute('height', 24 * z); pt.setAttribute('x', dx); pt.setAttribute('y', dy);
    pt.firstChild.setAttribute('cx', 12 * z); pt.firstChild.setAttribute('cy', 12 * z);
    document.querySelectorAll('[data-zoom]').forEach(function (e) { e.textContent = Math.round(z * 100) + ' %'; });
  }

  // Layout check with longer strings: ?lang=it swaps each text node that has an entry in window.NC_IT (it.js).
  function translate() {
    var T = window.NC_IT;
    if (!T || new URLSearchParams(location.search).get('lang') !== 'it') return;
    var w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT), n;
    while ((n = w.nextNode())) { var k = n.nodeValue.replace(/\s+/g, ' ').trim(); if (T[k]) n.nodeValue = n.nodeValue.replace(n.nodeValue.trim(), T[k]); }
  }

  document.addEventListener('DOMContentLoaded', function () {
    document.body.insertAdjacentHTML('afterbegin', sprite);
    shell();
    document.querySelectorAll('svg.pid[data-loop]').forEach(loop);
    document.querySelectorAll('.gview[data-flow]').forEach(flow);
    translate();
    document.querySelectorAll('.tabs').forEach(function (t) { // tabs that do not fit go into a "More" menu
      var more = null;
      while (t.scrollWidth > t.clientWidth && t.children.length > 2) {
        if (!more) { more = document.createElement('span'); more.innerHTML = (window.NC_IT && /lang=it/.test(location.search) ? 'Altro' : 'More') + ' <i data-i="down"></i>'; t.appendChild(more); }
        t.removeChild(more.previousElementSibling);
      }
    });
    document.querySelectorAll('i[data-i]').forEach(function (e) { e.outerHTML = icon(e.dataset.i, e.className); });
    // ?pop=add opens the node list popover of the Graph editor under its button. The popover exists below 1280 px only.
    var add = document.querySelector('.gbar .addn'), pal = document.querySelector('.gpal');
    if (add && add.offsetWidth && new URLSearchParams(location.search).get('pop') === 'add') {
      var r = add.getBoundingClientRect();
      add.classList.add('on'); pal.classList.add('open'); pal.style.left = r.left + 'px'; pal.style.top = r.bottom + 4 + 'px';
    }
  });
})();
