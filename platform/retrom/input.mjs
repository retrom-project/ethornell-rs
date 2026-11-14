// Retrom additions: MPL-2.0.
export function attachInput(canvas, send, gesture) {
  const bindings = [], keys = new Set();
  const listen = (target, name, handler, options) => {target.addEventListener(name, handler, options);
    bindings.push(() => target.removeEventListener(name, handler, options));};
  const point = event => {const r = canvas.getBoundingClientRect();
    return [(event.clientX - r.left) * canvas.width / Math.max(r.width, 1), (event.clientY - r.top) * canvas.height / Math.max(r.height, 1)];};
  for (const type of ["pointermove", "pointerdown", "pointerup"]) listen(canvas, type, event => {
    const [x, y] = point(event), code = event.button === 0 ? 1 : event.button === 2 ? 2 : 0;
    if (type === "pointerdown") {canvas.focus(); if (event.isTrusted) canvas.setPointerCapture(event.pointerId); gesture();}
    if (type === "pointermove") send([1, x, y, 0, 0]);
    else if (code) {send([1, x, y, 0, 0]); send([type === "pointerdown" ? 2 : 3, x, y, code, 0]);}
    event.preventDefault();
  });
  const release = () => {keys.clear(); send([7, 0, 0, 0, 0]);};
  listen(canvas, "pointercancel", release); listen(window, "blur", release);
  listen(canvas, "contextmenu", event => event.preventDefault());
  listen(canvas, "wheel", event => {const [x, y] = point(event); send([6, x, y, 0, -Math.sign(event.deltaY)]); event.preventDefault();}, {passive: false});
  for (const type of ["keydown", "keyup"]) listen(window, type, event => {
    if (event.isComposing || event.metaKey || !event.keyCode) return;
    const down = type === "keydown";
    if (down) {keys.add(event.keyCode); gesture();} else keys.delete(event.keyCode);
    send([down ? 4 : 5, 0, 0, event.keyCode, 0]); event.preventDefault();
  });
  const dispose = () => {release(); for (const remove of bindings) remove();};
  dispose.release = release; return dispose;
}
