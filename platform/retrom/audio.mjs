// Retrom additions: MPL-2.0. PCM stays on the worker/worklet connection.
export async function openAudio(workletURL) {
  const context = new AudioContext(), gain = context.createGain();
  try {
    await context.audioWorklet.addModule(workletURL);
    const node = new AudioWorkletNode(context, "ethornell-output", {outputChannelCount: [2]});
    node.connect(gain); gain.connect(context.destination);
    const channel = new MessageChannel(); node.port.postMessage({port: channel.port1}, [channel.port1]);
    return {port: channel.port2, sampleRate: context.sampleRate, resume: () => context.resume(), pause: () => context.suspend(),
      volume: value => {gain.gain.value = Math.max(0, Math.min(1, value));},
      close: async () => {node.disconnect(); gain.disconnect(); channel.port2.close(); await context.close();}};
  } catch (error) {await context.close(); throw error;}
}
