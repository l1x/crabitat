import { store } from './store';
import { renderSnapshot, renderCrabUpdated, renderColonyCreated, renderMissionCreated, renderTaskCreated, renderTaskUpdated, renderRunUpdated } from './render';
import type { ConsoleEvent } from '../lib/types';

const WS_PORT = 8800;
const RECONNECT_DELAY = 3000;

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

function setConnectionStatus(status: 'live' | 'reconnecting' | 'offline') {
  const el = document.getElementById('connection-status');
  if (!el) return;

  el.className = 'footer-status';
  switch (status) {
    case 'live':
      el.classList.add('footer-status--live');
      el.textContent = 'LIVE';
      break;
    case 'reconnecting':
      el.classList.add('footer-status--reconnecting');
      el.textContent = 'RECONNECTING';
      break;
    case 'offline':
      el.classList.add('footer-status--offline');
      el.textContent = 'OFFLINE';
      break;
  }
}

function handleEvent(event: ConsoleEvent) {
  switch (event.type) {
    case 'snapshot':
      store.init(event);
      renderSnapshot();
      break;
    case 'crab_updated':
      store.updateCrab(event.crab);
      renderCrabUpdated(event.crab);
      break;
    case 'colony_created':
      store.addColony(event.colony);
      renderColonyCreated(event.colony);
      break;
    case 'mission_created':
      store.addMission(event.mission);
      renderMissionCreated(event.mission);
      break;
    case 'task_created':
      store.addTask(event.task);
      renderTaskCreated(event.task);
      break;
    case 'task_updated':
      store.updateTask(event.task);
      renderTaskUpdated(event.task);
      break;
    case 'run_created':
      store.updateRun(event.run);
      renderRunUpdated(event.run);
      break;
    case 'run_updated':
      store.updateRun(event.run);
      renderRunUpdated(event.run);
      break;
    case 'run_completed':
      store.updateRun(event.run);
      renderRunUpdated(event.run);
      break;
  }
}

function connect() {
  const host = window.location.hostname || '127.0.0.1';
  const url = `ws://${host}:${WS_PORT}/v1/ws/console`;

  ws = new WebSocket(url);

  ws.addEventListener('open', () => {
    setConnectionStatus('live');
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  });

  ws.addEventListener('message', (msg) => {
    try {
      const event: ConsoleEvent = JSON.parse(msg.data);
      handleEvent(event);
    } catch {
      // ignore malformed messages
    }
  });

  ws.addEventListener('close', () => {
    setConnectionStatus('reconnecting');
    scheduleReconnect();
  });

  ws.addEventListener('error', () => {
    ws?.close();
  });
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, RECONNECT_DELAY);
}

// Start connection
connect();
