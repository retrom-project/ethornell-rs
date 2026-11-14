// Retrom additions: MPL-2.0. Native host requests stay inside the player frame.
export async function showDialog(message, command) {
  const value = JSON.parse(message.payload || "{}"), dialog = document.createElement("dialog");
  const title = document.createElement("h2"), text = document.createElement("p"), fields = [], actions = document.createElement("div");
  title.textContent = value.title || ""; text.textContent = value.prompt || message.text || "";
  dialog.style.cssText = "max-width:min(90vw,36rem);max-height:90vh;overflow:auto;background:#171923;color:#fff;border:1px solid #777;border-radius:8px;padding:24px;font:16px sans-serif";
  dialog.append(title, text);
  const input = (label, initial, maximum, numeric) => {
    const wrapper = document.createElement("label"), field = document.createElement("input");
    wrapper.textContent = label || ""; wrapper.style.display = "block";
    field.value = initial || ""; if (maximum) field.maxLength = maximum; if (numeric) field.inputMode = "numeric";
    field.style.cssText = "display:block;padding:8px;margin:8px 0;color:#111;background:#fff";
    wrapper.append(field); dialog.append(wrapper); fields.push(field); return field;
  };
  const response = await new Promise(resolve => {
    const button = (label, result) => {
      const element = document.createElement("button"); element.textContent = label;
      element.style.cssText = "padding:8px 16px;margin:4px;cursor:pointer";
      element.onclick = () => resolve(typeof result === "function" ? result() : result); actions.append(element);
    };
    dialog.oncancel = event => {event.preventDefault(); if (message.kind !== 2) resolve({accepted: false});};
    if (message.kind === 2) {
      if (message.code !== 0) button("No", 0); button(message.code === 0 ? "OK" : "Yes", 1);
    } else {
      button("Cancel", {accepted: false});
      if (value.type === "input") for (const field of value.fields) input(field.label, field.initial, field.max_bytes, field.numeric);
      else if (value.type === "segmented") for (let i = 0; i < value.segments; i++) input(String(i + 1), "", value.max_bytes, value.numeric);
      else if (value.type === "date") {
        for (const initial of value.fields) input("", initial, 10, false);
        input("Month", String(value.month + 1), 2, true); input("Day", String(value.day + 1), 2, true);
      } else if (value.type === "select") value.options.forEach((label, index) => button(label, {accepted: true, index}));
      else {resolve({accepted: false}); return;}
      if (value.type !== "select") button("OK", () => value.type === "date"
        ? {accepted: true, texts: fields.slice(0, -2).map(field => field.value), month: Number(fields.at(-2).value) - 1, day: Number(fields.at(-1).value) - 1}
        : {accepted: true, texts: fields.map(field => field.value)});
    }
    dialog.append(actions); document.body.append(dialog); dialog.showModal();
  });
  dialog.close(); dialog.remove();
  if (message.kind === 2) await command("answer", {dialogId: message.id, result: response});
  else await command("dialog", {dialogId: message.id, response: JSON.stringify(response)});
}
