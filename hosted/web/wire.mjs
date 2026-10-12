export class DeviceWire {
  constructor({ config, api, deviceId, onEvent, onState }) {
    Object.assign(this, { config, api, deviceId, onEvent, onState });
    this.waiting = new Map();
    this.stopped = false;
    this.connecting = false;
    this.attempt = 0;
    this.rtts = [];
  }
  async connect() {
    if (
      this.connecting ||
      this.stopped ||
      this.socket?.readyState === WebSocket.OPEN
    )
      return;
    this.connecting = true;
    this.onState(false, "Connecting");
    try {
      const { ticket } = await this.api("/ticket", "POST", {
        deviceId: this.deviceId,
      });
      if (this.stopped) return;
      const url = new URL(this.config.websocket);
      url.searchParams.set("ticket", ticket);
      const socket = new WebSocket(url);
      this.socket = socket;
      const timeout = setTimeout(() => socket.close(), 10000);
      socket.onopen = () => {
        clearTimeout(timeout);
        if (this.stopped || this.socket !== socket) {
          socket.close();
          return;
        }
        this.attempt = 0;
        this.onState(true, "Connected");
        socket.send(JSON.stringify({ action: "hello" }));
        for (const pending of this.waiting.values())
          socket.send(pending.payload);
        this.onEvent({ type: "reconnected" });
      };
      socket.onmessage = ({ data }) => {
        if (this.stopped || this.socket !== socket) return;
        let event;
        try {
          event = JSON.parse(data);
        } catch {
          return;
        }
        if (event.type === "ack") {
          const key = `${event.sessionId}:${event.sequence}`,
            waiting = this.waiting.get(key);
          if (waiting) {
            this.rtts.push(performance.now() - waiting.started);
            this.rtts = this.rtts.slice(-200);
            waiting.resolve(event);
            this.waiting.delete(key);
          }
        } else if (event.type === "error") {
          for (const pending of this.waiting.values())
            pending.reject(new Error(event.message));
          this.waiting.clear();
          this.onEvent(event);
          if ([401, 403].includes(event.status)) socket.close();
        } else this.onEvent(event);
      };
      socket.onerror = () => {
        this.onState(false, "Reconnecting");
      };
      socket.onclose = () => {
        clearTimeout(timeout);
        if (this.stopped) return;
        this.onState(false, "Reconnecting");
        this.onEvent({ type: "disconnected" });
        if (!this.stopped)
          this.reconnect = setTimeout(
            () => this.connect(),
            Math.min(20000, 500 * 2 ** this.attempt++) + Math.random() * 300,
          );
      };
      clearInterval(this.heartbeat);
      this.heartbeat = setInterval(() => {
        if (socket.readyState === WebSocket.OPEN)
          socket.send(JSON.stringify({ action: "ping", sentAt: Date.now() }));
      }, 60000);
    } catch (error) {
      this.onState(false, "Offline");
      this.onEvent({ type: "error", message: error.message });
      if (!this.stopped)
        this.reconnect = setTimeout(
          () => this.connect(),
          Math.min(20000, 1000 * 2 ** this.attempt++),
        );
    } finally {
      this.connecting = false;
    }
  }
  async publish(message) {
    if (this.stopped)
      throw new Error(
        "This device is disconnected. Save or export your draft.",
      );
    const key = `${message.sessionId}:${message.sequence}`;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.waiting.delete(key);
        reject(
          new Error(
            "Live delivery was not confirmed. Save your draft to share it through history.",
          ),
        );
      }, 45000);
      const payload = JSON.stringify({ ...message, action: "stream" });
      this.waiting.set(key, {
        payload,
        started: performance.now(),
        resolve: (receipt) => {
          clearTimeout(timeout);
          resolve(receipt);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
      });
      try {
        if (this.socket?.readyState === WebSocket.OPEN)
          this.socket.send(payload);
      } catch (error) {
        this.waiting.delete(key);
        clearTimeout(timeout);
        reject(error);
      }
    });
  }
  close() {
    this.stopped = true;
    for (const pending of this.waiting.values())
      pending.reject(new Error("Live delivery cancelled on this device."));
    this.waiting.clear();
    clearInterval(this.heartbeat);
    clearTimeout(this.reconnect);
    this.socket?.close();
  }
}
