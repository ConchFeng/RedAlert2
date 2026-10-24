import test from 'node:test';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { LobbyService, SqlStore } from './service.js';

function fixture(bodyTimeoutMs = 5000) {
  const db = new DatabaseSync(':memory:');
  const sql = { exec(query, ...args) {
    const statement = db.prepare(query);
    if (/^\s*SELECT/i.test(query)) return { toArray: () => statement.all(...args) };
    statement.run(...args); return { toArray: () => [] };
  } };
  const store = new SqlStore(sql, (fn) => {
    db.exec('BEGIN');
    try { const result = fn(); db.exec('COMMIT'); return result; }
    catch (error) { db.exec('ROLLBACK'); throw error; }
  });
  let now = 100000;
  const env = { RELAY_TICKET_KEY: 'ab'.repeat(32), RELAY_ADDRESS: '192.0.2.10:42000' };
  let service = new LobbyService(store, env, () => now, bodyTimeoutMs);
  let requestId = 0;
  const cursors = new Map();
  async function request(path, token, body) {
    if (body && (path === '/v1/accounts' || path === '/v1/sessions') && body.protocol === undefined) body = { ...body, protocol: 3 };
    const response = await service.fetch(new Request(`https://lobby.invalid${path}`, {
      method: body ? 'POST' : 'GET', headers: { 'content-type': 'application/json',
        ...(token ? { authorization: `Bearer ${token}` } : {}) },
      ...(body ? { body: JSON.stringify(body) } : {}),
    }));
    const value = await response.json();
    if (token && value.cursor !== undefined) cursors.set(token, Math.max(cursors.get(token) ?? 0, value.cursor));
    return { status: response.status, body: value };
  }
  async function account(nickname) {
    const result = await request('/v1/accounts', null, { nickname, password: 'long-test-password' });
    assert.equal(result.status, 200); return result.body;
  }
  const command = async (user, operation, payload = {}, extra = {}) => {
    const result = await request('/v1/commands', user.token,
      { request_id: `request_${++requestId}`, after: cursors.get(user.token) ?? 0, operation, payload, ...extra });
    if (result.status === 200 && operation !== 'logout') {
      const remaining = await request(`/v1/events?after=${cursors.get(user.token) ?? 0}`, user.token);
      assert.equal(remaining.status, 200);
      result.body.events.push(...remaining.body.events);
      result.body.cursor = remaining.body.cursor;
    }
    return result;
  };
  return { account, command, request, store, advance: (seconds) => { now += seconds; },
    raw: (request) => service.fetch(request),
    parse: (body) => service.readBody(new Request('https://lobby.invalid/', { method: 'POST',
      headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })),
    emit: (user, kind, payload) => service.emit(store.list('session:').find((session) => session.player_id === user.player_id).id, kind, payload),
    restart: () => { service = new LobbyService(store, env, () => now); } };
}

const channel = { name: 'Test Room', kind: 41, max_users: 30, flags: 0, game_flags: 0, ex_info: 'test' };
const slot = { country: 0, country_random: true, color_index: 0, color_random: true, start_position: 'Auto', team: 'None' };
const human = (name, row = 0, observer = false) => ({ name, raw_slot_index: row, slot: { ...slot }, role: observer ? 'Observer' : { Player: { group: 0 } } });
const options = { launch: { local_player_index: 0, humans: [human('Host'), human('Guest', 1)],
  mode: { class: 'Battle', id: 1, ui_name_key: 'TXT_BATTLE', tooltip_key: '', override_file: 'MPBattleMD.ini', map_filter: 'standard',
    random_maps_allowed: true, allies_allowed: true, must_ally: false },
  selected_map_file: 'Test.yrm', ai_player_count: 0, opponents: [],
  pre_fill_house_roster: { human_nodes: [{ priority: 0, source_order: 0, observer: false }], ai_slots: [], fixed_tail: ['Neutral', 'Special'] },
  options: { starting_credits: 10000, unit_count: 10, tech_level: 10, game_speed: 0, default_ai_difficulty: 1,
    short_game: true, bases: true, bridges_destroyable: true, super_weapons: true, build_off_ally: true, crates: false,
    mcv_redeploy: true, fog_of_war: false, shroud: true, tiberium_grows: true, multi_engineer: false, harvester_truce: false, ally_change_allowed: true } },
  map_metadata: { display_name: 'Test map', player_capacity: 30, official: false, byte_length: 120, digest: 'map-one', source_sha256: '11'.repeat(32), effective_digest: 'aa'.repeat(32) },
  vacancies: Array(30).fill('open'), observer_row: null, cooperative_difficulty: 1, seed: null };
const agreement = (revision) => ({ revision, version: 'test-v1', rules_digest: 'rules-one', map_digest: '11'.repeat(32), effective_map_digest: 'aa'.repeat(32) });

async function room(f, host, guest) {
  assert.equal((await f.command(host, 'create', { channel, key: '' })).status, 200);
  assert.equal((await f.command(guest, 'join', { channel: channel.name, key: '' })).status, 200);
  const result = await f.command(host, 'options', { options });
  assert.equal(result.status, 200);
  const revision = result.body.events.filter((event) => event.kind === 'options').at(-1).payload.revision;
  for (const user of [host, guest]) {
    assert.equal((await f.command(user, 'map_status', { status: 1, ...agreement(revision) })).status, 200);
    assert.equal((await f.command(user, 'ready', { ready: true, ...agreement(revision) })).status, 200);
  }
  return revision;
}

test('public chat accepts the legacy client lobby key without weakening game-room keys', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const publicRoom = (await f.request('/v1/channels?kind=0', guest.token)).body.channels[0];
  const joined = await f.command(guest, 'join', { channel: publicRoom.name, key: 'legacy-client-lobby-key' });
  assert.equal(joined.status, 200);
  assert.equal(joined.body.events.find((event) => event.kind === 'joined').payload.channel.id, 'public');
  assert.equal((await f.command(host, 'create', { channel, key: 'protected-room-key' })).status, 200);
  assert.equal((await f.command(guest, 'join', { channel: channel.name, key: 'wrong-key' })).status, 403);
});

test('host can create a same-name rematch before the former guest leaves results', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const newcomer = await f.account('NextGuest');
  const revision = await room(f, host, guest);
  const start = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  const match = start.body.events.find((event) => event.kind === 'game_started').payload.match_id;
  assert.equal((await f.command(newcomer, 'create', { channel, key: '' })).status, 409,
    'a connecting room keeps its name because start can still abort');
  for (const user of [host, guest]) await f.command(user, 'relay_ready', { match_id: match });
  assert.equal((await f.command(newcomer, 'create', { channel, key: '' })).status, 409,
    'a probing room keeps its name because start can still abort');
  await f.command(host, 'latency_ready', { match_id: match, pings: [{ source_index: 1, round_trip_ms: 100 }] });
  await f.command(guest, 'latency_ready', { match_id: match, pings: [{ source_index: 0, round_trip_ms: 100 }] });
  for (const user of [host, guest]) await f.command(user, 'leave');
  assert.equal((await f.command(host, 'finish', { match_id: match })).status, 200);
  const rematch = await f.command(host, 'create', { channel, key: '' });
  assert.equal(rematch.status, 200);
  const newRoom = rematch.body.events.find((event) => event.kind === 'created').payload.channel.id;
  assert.ok(f.store.get(`match:${match}`), 'the guest retains its original match lease');
  const earlyJoin = await f.command(newcomer, 'join', { channel: channel.name, key: '' });
  assert.equal(earlyJoin.status, 200);
  assert.equal(earlyJoin.body.events.find((event) => event.kind === 'joined').payload.channel.id, newRoom);
  assert.equal((await f.command(guest, 'finish', { match_id: match })).status, 200);
  const joined = await f.command(guest, 'join', { channel: channel.name, key: '' });
  assert.equal(joined.status, 200);
  assert.equal(joined.body.events.find((event) => event.kind === 'joined').payload.channel.id, newRoom);
  assert.ok(f.store.get(`room:${newRoom}`), 'old-match cleanup preserves the new lobby');
  assert.equal(f.store.get(`match:${match}`), null);
});

test('real room agreement gates relay admission, survives actor restart and gives rematch fresh tickets', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  let revision = await room(f, host, guest);
  f.restart();
  const start = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  assert.equal(start.status, 200);
  const begun = start.body.events.find((event) => event.kind === 'game_started').payload;
  const firstTicket = Buffer.from(begun.relay.ticket, 'base64url');
  assert.equal(firstTicket.length, 98);
  assert.equal(firstTicket[0], 2);
  assert.equal(firstTicket.readBigUInt64LE(18), 100120n);
  assert.equal(firstTicket.readBigUInt64LE(58), 1n);
  assert.equal(firstTicket.subarray(42, 58).toString('hex'), host.player_id.replaceAll('-', ''));
  assert.equal(Buffer.from(begun.relay.packet_key, 'base64url').length, 32);
  assert.equal(Buffer.from(begun.relay.payload_key, 'base64url').length, 32);
  assert.equal(start.body.events.some((event) => event.kind === 'start_confirmed'), false);
  const firstReady = await f.command(host, 'relay_ready', { match_id: begun.match_id });
  assert.equal(firstReady.body.events.some((event) => event.kind === 'start_confirmed'), false);
  await f.command(guest, 'relay_ready', { match_id: begun.match_id });
  assert.equal((await f.request('/v1/events?after=0', host.token)).body.events.some((event) => event.kind === 'start_confirmed'), true);
  const hostTiming = await f.command(host, 'latency_ready', { match_id: begun.match_id, pings: [{ source_index: 1, round_trip_ms: 900 }] });
  assert.equal(hostTiming.body.events.some((event) => event.kind === 'timing_confirmed'), false);
  await f.command(guest, 'latency_ready', { match_id: begun.match_id, pings: [{ source_index: 0, round_trip_ms: 1400 }] });
  const timing = (await f.request('/v1/events?after=0', host.token)).body.events.find((event) => event.kind === 'timing_confirmed');
  assert.deepEqual(timing.payload.pings, [1400, 900]);
  assert.equal(timing.payload.relay.payload_key, begun.relay.payload_key);
  assert.equal(Buffer.from(timing.payload.relay.ticket, 'base64url').readBigUInt64LE(18), 100300n);
  assert.equal(Buffer.from(timing.payload.relay.ticket, 'base64url').readBigUInt64LE(58), 1n);
  f.advance(1);
  assert.equal((await f.command(host, 'renew_relay', { match_id: begun.match_id, source_index: 1 })).status, 400);
  const renewed = await f.command(host, 'renew_relay', { match_id: begun.match_id });
  assert.equal(renewed.status, 200);
  const lease = renewed.body.events.find((event) => event.kind === 'relay_renewed').payload;
  assert.equal(lease.match_id, begun.match_id);
  assert.equal(lease.relay.source_index, 0, 'only the authenticated participant can renew its own source');
  assert.notEqual(lease.relay.packet_key, begun.relay.packet_key);
  assert.equal(lease.relay.payload_key, begun.relay.payload_key);
  assert.equal(Buffer.from(lease.relay.ticket, 'base64url').readBigUInt64LE(18),
    Buffer.from(begun.relay.ticket, 'base64url').readBigUInt64LE(18) + 181n);
  for (const user of [host, guest]) {
    await f.command(user, 'leave');
    assert.equal((await f.command(user, 'finish', { match_id: begun.match_id })).status, 200);
  }
  assert.equal((await f.command(host, 'renew_relay', { match_id: begun.match_id })).status, 409);
  revision = await room(f, host, guest);
  const next = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  const rematch = next.body.events.filter((event) => event.kind === 'game_started').at(-1).payload;
  assert.notEqual(rematch.match_id, begun.match_id);
  assert.notEqual(rematch.relay.packet_key, begun.relay.packet_key);
  assert.notEqual(rematch.relay.payload_key, begun.relay.payload_key);
  assert.equal(Buffer.from(rematch.relay.ticket, 'base64url').readBigUInt64LE(58), 2n);
});

test('latency barrier rejects missing peers and cancellation frees every pending admission', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const revision = await room(f, host, guest);
  const started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  const id = started.body.events.find((event) => event.kind === 'game_started').payload.match_id;
  assert.equal((await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 1, round_trip_ms: 10 }] })).status, 409);
  for (const user of [host, guest]) await f.command(user, 'relay_ready', { match_id: id });
  assert.equal((await f.command(host, 'latency_ready', { match_id: id, pings: [] })).status, 400);
  assert.equal((await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 0, round_trip_ms: 10 }] })).status, 400);
  await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 1, round_trip_ms: 10 }] });
  assert.equal((await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 1, round_trip_ms: 20 }] })).status, 409);
  await f.command(guest, 'finish', { match_id: id });
  assert.equal(f.store.list('match:').length, 0);
  assert.equal(f.store.list('session:').every((session) => session.match_id === null), true);
  assert.equal(f.store.list('room:').find((room) => room.kind === 41).phase, 'lobby');
});

test('authenticated ownership, stale agreement and idempotency cannot alter another player or repeat side effects', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const revision = await room(f, host, guest);
  assert.equal((await f.request('/v1/channels?kind=41')).status, 401);
  assert.equal((await f.command(guest, 'options', { options })).status, 403);
  assert.equal((await f.command(guest, 'ready', { ready: true, ...agreement(revision - 1) })).status, 409);
  const change = { ...agreement(revision), rules_digest: 'wrong-rules', ready: true };
  await f.command(guest, 'ready', change);
  assert.equal((await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) })).status, 409);
  const chat = { request_id: 'same-request', operation: 'chat', payload: { text: 'hello', to: [], action: false } };
  await f.request('/v1/commands', host.token, chat);
  await f.request('/v1/commands', host.token, chat);
  const events = (await f.request('/v1/events?after=0', guest.token)).body.events;
  assert.equal(events.filter((event) => event.kind === 'chat').length, 1);
  chat.payload.text = 'different';
  assert.equal((await f.request('/v1/commands', host.token, chat)).status, 409);
  const badLogin = await f.request('/v1/sessions', null, { nickname: 'Host', password: 'different-password' });
  assert.equal(badLogin.status, 401);
  const raw = JSON.stringify(f.store.list('account:'));
  assert.equal(raw.includes('long-test-password'), false);
  assert.equal(JSON.stringify(f.store.list('session:')).includes(host.token), false);
});

test('relay connection timeout returns participants to editable room and stale event cursors fail explicitly', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const revision = await room(f, host, guest);
  await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  f.advance(31);
  const timeout = await f.request('/v1/events?after=0', guest.token);
  assert.equal(timeout.status, 200);
  assert.equal(timeout.body.events.some((event) => event.kind === 'start_rejected'), true);
  assert.equal((await f.command(guest, 'ready', { ready: true, ...agreement(revision) })).status, 409);
  for (let i = 0; i < 1025; i++) {
    f.emit(guest, 'error', { code: 'fixture', message: 'critical' });
  }
  assert.equal((await f.request('/v1/events?after=0', guest.token)).status, 409);
  const before = JSON.stringify(f.store.list('room:'));
  assert.equal((await f.command(guest, 'leave', {}, { after: 0 })).status, 409);
  assert.equal(JSON.stringify(f.store.list('room:')), before, 'invalid cursors cannot commit a command');
  f.advance(301);
  await f.request('/v1/events?after=0', host.token);
  assert.equal(f.store.list('rate:').some((bucket) => bucket.until <= 100332), false);
});

test('signed-in players behind one NAT have independent command budgets', async () => {
  const f = fixture(); const left = await f.account('Left'); const right = await f.account('Right');
  for (let i = 0; i < 121; i++) {
    for (const user of [left, right]) {
      const result = await f.request('/v1/commands', user.token,
        { request_id: `ping_${i}`, after: 0, operation: 'ping', payload: {} });
      assert.equal(result.status, 200);
    }
  }
});

test('thirty-player setup traffic delivers final options within a bounded control window', async () => {
  const f = fixture(); const users = [await f.account('Host')];
  for (let i = 1; i < 30; i++) users.push(await f.account(`Guest${i}`));
  const host = users[0], humans = [human('Host')], received = [];
  let cursor = 0, requestId = 0;
  const snapshot = () => ({ ...options, seed: 123, vacancies: Array(30).fill('open'),
    launch: { ...options.launch, humans: [...humans] } });
  async function command(operation, payload = {}) {
    const result = await f.request('/v1/commands', host.token,
      { request_id: `setup30_${++requestId}`, after: cursor, operation, payload });
    assert.equal(result.status, 200, `${operation} remains admitted during thirty-player setup`);
    cursor = result.body.cursor; received.push(...result.body.events);
  }
  async function proveOptions() {
    for (const event of received.splice(0)) if (event.kind === 'options') {
      await command('map_status', { status: 1, ...agreement(event.payload.revision) });
      await command('ready', { ready: true, ...agreement(event.payload.revision) });
    }
  }
  await command('create', { channel, key: '' });
  await command('options', { options: snapshot() }); await proveOptions();
  await command('advertise', { channel }); await command('ping');
  for (let i = 1; i < users.length; i++) {
    f.advance(1);
    assert.equal((await f.command(users[i], 'join', { channel: channel.name, key: '' })).status, 200);
    humans.push(human(users[i].nickname, i));
    // Ordinary join producer: roster/options, ping, advertisement; one worker poll per second.
    await command('options', { options: snapshot() });
    await command('options', { options: snapshot() });
    await command('ping'); await command('advertise', { channel });
    const polled = await f.request(`/v1/events?after=${cursor}`, host.token);
    assert.equal(polled.status, 200); cursor = polled.body.cursor;
    received.push(...polled.body.events); await proveOptions();
    if (i % 5 === 0) await command('ping');
  }
  const delivered = await f.request('/v1/events?after=0', users.at(-1).token);
  assert.equal(delivered.status, 200);
  const final = delivered.body.events.filter((event) => event.kind === 'options').at(-1).payload.options;
  assert.deepEqual(final.launch.humans, humans); assert.equal(final.seed, 123);
  let limited = false;
  for (let i = 0; i < 600 && !limited; i++) {
    const result = await f.request(`/v1/events?after=${cursor}`, host.token);
    assert.ok(result.status === 200 || result.status === 429); limited = result.status === 429;
  }
  assert.equal(limited, true, 'the per-session control bucket remains bounded');
});

test('chat is action-limited and cannot evict an unread critical event', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  await room(f, host, guest);
  for (let i = 0; i < 30; i++) {
    const result = await f.request('/v1/commands', host.token,
      { request_id: `chat_${i}`, operation: 'chat', payload: { text: 'hello', to: [], action: false } });
    assert.equal(result.status, 200);
  }
  assert.equal((await f.command(host, 'chat', { text: 'limited', to: [], action: false })).status, 429);
  f.emit(guest, 'start_rejected', { message: 'critical retained' });
  for (let i = 0; i < 1100; i++) f.emit(guest, 'chat', { text: 'bounded soft event' });
  const observed = await f.request('/v1/events?after=0', guest.token);
  assert.equal(observed.status, 200);
  assert.equal(observed.body.events.filter((event) => event.kind === 'chat').length, 64);
  assert.equal(observed.body.events.some((event) => event.kind === 'start_rejected'), true);
  assert.equal((await f.request(`/v1/events?after=${observed.body.cursor}`, guest.token)).body.events.length, 0);
});

test('cancelling a pending start aborts every admission and frees the departed host room', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  let revision = await room(f, host, guest);
  let started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  let id = started.body.events.find((event) => event.kind === 'game_started').payload.match_id;
  await f.command(guest, 'relay_ready', { match_id: id });
  await f.command(guest, 'finish', { match_id: id });
  assert.equal((await f.command(host, 'relay_ready', { match_id: id })).status, 409);
  let observed = await f.request('/v1/events?after=0', host.token);
  assert.equal(observed.body.events.some((event) => event.kind === 'start_confirmed'), false);
  revision = observed.body.events.filter((event) => event.kind === 'options').at(-1).payload.revision;
  for (const user of [host, guest]) {
    await f.command(user, 'map_status', { status: 1, ...agreement(revision) });
    await f.command(user, 'ready', { ready: true, ...agreement(revision) });
  }
  started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  id = started.body.events.filter((event) => event.kind === 'game_started').at(-1).payload.match_id;
  await f.command(host, 'leave');
  assert.equal((await f.command(guest, 'relay_ready', { match_id: id })).status, 409);
  assert.equal(f.store.list('room:').some((room) => room.channel.name === channel.name), false);
  assert.equal(f.store.list('match:').length, 0);
});

test('unfinished unauthenticated body cannot block another client and times out', async () => {
  const f = fixture(30);
  const body = new ReadableStream({ start(controller) { controller.enqueue(new TextEncoder().encode('{')); } });
  let settled = false;
  const pending = f.raw(new Request('https://lobby.invalid/v1/accounts', {
    method: 'POST', headers: { 'content-type': 'application/json' }, body, duplex: 'half',
  })).then((result) => { settled = true; return result; });
  const health = await f.request('/health');
  assert.equal(health.status, 200);
  assert.equal(settled, false);
  assert.equal((await pending).status, 408);
});

test('crashed clients release their account and match membership while active peers can finish', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  const revision = await room(f, host, guest);
  const started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  const id = started.body.events.find((event) => event.kind === 'game_started').payload.match_id;
  for (const user of [host, guest]) await f.command(user, 'relay_ready', { match_id: id });
  await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 1, round_trip_ms: 30 }] });
  await f.command(guest, 'latency_ready', { match_id: id, pings: [{ source_index: 0, round_trip_ms: 40 }] });
  assert.equal((await f.request('/v1/sessions', null, { nickname: 'Guest', password: 'long-test-password' })).status, 409);
  f.advance(121);
  assert.equal((await f.command(host, 'ping')).status, 200);
  assert.deepEqual(f.store.list('match:')[0].finished, [guest.player_id]);
  assert.equal((await f.request('/v1/sessions', null, { nickname: 'Guest', password: 'long-test-password' })).status, 200);
  await f.command(host, 'finish', { match_id: id });
  assert.equal(f.store.list('match:').length, 0);
  const leave = await f.command(host, 'leave');
  assert.equal(leave.body.events.some((event) => event.kind === 'channel_left'), true);
  assert.equal(f.store.list('session:').find((session) => session.player_id === host.player_id).channel_id, null);
});

test('room map transfer checks ownership, ordering, bytes and cleanup independently of roster revision', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  await room(f, host, guest);
  const bytes = Buffer.alloc(35000, 7);
  const digest = Buffer.from(await crypto.subtle.digest('SHA-256', bytes)).toString('hex');
  const selected = { ...options, map_metadata: { ...options.map_metadata, byte_length: bytes.length, source_sha256: digest } };
  await f.command(host, 'options', { options: selected });
  const chunk = (offset, value = bytes.subarray(offset, offset + 32768)) =>
    ({ digest, byte_length: bytes.length, offset, data: value.toString('base64url') });
  assert.equal((await f.request(`/v1/maps/${digest}?offset=0`, guest.token)).status, 409);
  assert.equal((await f.command(guest, 'map_upload', chunk(0))).status, 403);
  assert.equal((await f.command(host, 'map_upload', chunk(32768))).status, 409);
  assert.equal((await f.command(host, 'map_upload', chunk(0))).status, 200);
  assert.equal((await f.command(host, 'map_upload', chunk(0))).status, 200, 'identical chunk retry is safe');
  assert.equal((await f.command(host, 'map_upload', chunk(32768, Buffer.alloc(2232, 8)))).status, 400);
  const done = await f.command(host, 'map_upload', chunk(32768));
  assert.equal(done.status, 200);
  assert.equal(done.body.events.some((event) => event.kind === 'map_available'), true);
  // A settings revision with the same map keeps the already-verified bytes.
  await f.command(host, 'options', { options: { ...selected, cooperative_difficulty: 2 } });
  const received = [];
  for (const offset of [0, 32768]) {
    const response = await f.request(`/v1/maps/${digest}?offset=${offset}`, guest.token);
    assert.equal(response.status, 200);
    assert.equal(response.body.complete, offset === 32768);
    received.push(Buffer.from(response.body.data, 'base64url'));
  }
  assert.deepEqual(Buffer.concat(received), bytes);
  assert.equal((await f.request(`/v1/maps/${'22'.repeat(32)}?offset=0`, guest.token)).status, 409);
  await f.command(host, 'leave');
  assert.equal(f.store.list('map-upload:').length, 0);
  assert.equal(f.store.list('map-chunk:').length, 0);
  assert.equal((await f.request(`/v1/maps/${digest}?offset=0`, guest.token)).status, 409);
});

test('map capacity counts playing humans and AI while retaining the observer transport slot', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  await room(f, host, guest);
  const third = await f.account('Third');
  await f.command(third, 'join', { channel: channel.name, key: '' });
  const selected = { ...options, launch: { ...options.launch, humans: [
    human('Host', 0, true), human('Guest', 1), human('Third', 2),
  ] }, map_metadata: { ...options.map_metadata, player_capacity: 2 } };
  assert.equal((await f.command(host, 'options', { options: selected })).status, 200);
  selected.launch.humans[0].role = { Player: { group: 0 } };
  assert.equal((await f.command(host, 'options', { options: selected })).status, 400);
  selected.launch.humans[0].role = 'Observer';
  selected.launch.opponents = [{ ...slot, raw_slot_index: 3, difficulty: 'Easy' }];
  assert.equal((await f.command(host, 'options', { options: selected })).status, 400);
});

test('generated maps bind independent seed and preview transfers to the agreed start staging', async () => {
  const f = fixture(); const host = await f.account('Host'); const guest = await f.account('Guest');
  await room(f, host, guest);
  const seed = Buffer.from('[General]\nNumPlayers=2\nSeed=123\n');
  const preview = Buffer.alloc(33000, 9);
  const hash = async (bytes) => Buffer.from(await crypto.subtle.digest('SHA-256', bytes)).toString('hex');
  const digest = await hash(seed); const previewDigest = await hash(preview);
  const staging = `W100,100,0,0,2,10,10,20,20,90,90,80,80${',0,0,0,0'.repeat(6)}`;
  const selected = { ...options, map_metadata: { ...options.map_metadata, player_capacity: 2, byte_length: seed.length, source_sha256: digest },
    random_map: { start_staging: staging, preview: { source_sha256: previewDigest, byte_length: preview.length } } };
  assert.equal((await f.command(host, 'options', { options: { ...selected, map_metadata: { ...selected.map_metadata, player_capacity: 30 } } })).status, 400);
  const result = await f.command(host, 'options', { options: selected });
  assert.equal(result.status, 200);
  const revision = result.body.events.filter((event) => event.kind === 'options').at(-1).payload.revision;
  const upload = (bytes, sha, offset = 0, resource) => ({ digest: sha, byte_length: bytes.length, offset,
    data: bytes.subarray(offset, offset + 32768).toString('base64url'), ...(resource ? { resource } : {}) });
  assert.equal((await f.command(host, 'map_upload', upload(seed, digest))).status, 200);
  assert.equal((await f.command(host, 'map_upload', upload(preview, previewDigest, 0, 'preview'))).status, 200);
  assert.equal((await f.request(`/v1/maps/${digest}?resource=preview&offset=0`, guest.token)).status, 409);
  assert.equal((await f.command(host, 'map_upload', upload(preview, previewDigest, 32768, 'preview'))).status, 200);
  const received = await f.request(`/v1/maps/${digest}?resource=preview&offset=32768`, guest.token);
  assert.equal(received.status, 200);
  assert.equal(received.body.digest, previewDigest);
  assert.deepEqual(Buffer.from(received.body.data, 'base64url'), preview.subarray(32768));
  assert.equal((await f.request(`/v1/maps/${previewDigest}?resource=preview&offset=0`, guest.token)).status, 409);
  const agreed = { ...agreement(revision), map_digest: digest, staging_digest: await hash(Buffer.from(staging)) };
  for (const user of [host, guest]) {
    await f.command(user, 'map_status', { status: 1, ...agreed });
    await f.command(user, 'ready', { ready: true, ...agreed });
  }
  assert.equal((await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreed, staging_digest: '' })).status, 409);
  assert.equal((await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreed, effective_map_digest: 'bb'.repeat(32) })).status, 409);
  assert.equal((await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreed })).status, 200);
});

test('protocol upgrade is mandatory and a new login cannot reset account action budgets', async () => {
  const f = fixture();
  assert.equal((await f.raw(new Request('https://lobby.invalid/v1/accounts', { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify({ nickname: 'Missing', password: 'long-test-password' }) }))).status, 426);
  assert.equal((await f.request('/v1/accounts', null, { nickname: 'Old', password: 'long-test-password', protocol: 1 })).status, 426);
  assert.equal((await f.request('/v1/accounts', null, { nickname: 'Unsafe', password: 'long-test-password', protocol: 2 })).status, 426);
  assert.equal((await f.request('/health')).body.protocol, 3);
  const user = await f.account('Player');
  const old = f.store.list('session:')[0]; delete old.protocol; f.store.put(`session:${old.id}`, old);
  assert.equal((await f.request('/v1/channels?kind=0', user.token)).status, 426);
  old.protocol = 2; f.store.put(`session:${old.id}`, old);
  assert.equal((await f.request('/v1/channels?kind=0', user.token)).status, 426);
  old.protocol = 3; f.store.put(`session:${old.id}`, old);
  await f.command(user, 'join', { channel: 'Lob_41_0', key: '' });
  for (let i = 0; i < 30; i++) assert.equal((await f.command(user, 'chat', { text: 'hello', to: [] })).status, 200);
  const renewed = await f.request('/v1/sessions', null, { nickname: 'Player', password: 'long-test-password' });
  assert.equal(renewed.status, 200);
  await f.command(renewed.body, 'join', { channel: 'Lob_41_0', key: '' });
  assert.equal((await f.command(renewed.body, 'chat', { text: 'still limited', to: [] })).status, 429);
});

test('upgrade retires a stored v2 active membership without deleting accounts or v3 peers', async () => {
  const f = fixture(), host = await f.account('Host'), guest = await f.account('Guest');
  const revision = await room(f, host, guest);
  const started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
  const id = started.body.events.find((event) => event.kind === 'game_started').payload.match_id;
  for (const user of [host, guest]) await f.command(user, 'relay_ready', { match_id: id });
  await f.command(host, 'latency_ready', { match_id: id, pings: [{ source_index: 1, round_trip_ms: 10 }] });
  await f.command(guest, 'latency_ready', { match_id: id, pings: [{ source_index: 0, round_trip_ms: 10 }] });
  const old = f.store.list('session:').find((session) => session.player_id === host.player_id);
  old.protocol = 2; f.store.put(`session:${old.id}`, old);
  const account = f.store.get(old.account_key);
  const replacement = await f.request('/v1/sessions', null, { nickname: 'Host', password: 'long-test-password' });
  assert.equal(replacement.status, 200); assert.equal(f.store.get(`session:${old.id}`), null);
  assert.deepEqual(f.store.get(old.account_key), account);
  const currentGuest = f.store.list('session:').find((session) => session.player_id === guest.player_id);
  assert.equal(currentGuest.protocol, 3); assert.equal(currentGuest.match_id, id);
  assert.deepEqual(f.store.get(`match:${id}`).finished, [host.player_id]);
  await f.command(guest, 'finish', { match_id: id });
  assert.equal(f.store.get(`match:${id}`), null);
});

test('cancelled admissions have short v2 tickets, durable increasing generations and a start budget', async () => {
  const f = fixture(), host = await f.account('Host'), guest = await f.account('Guest');
  let revision = await room(f, host, guest);
  let previousMatch, previousKey;
  for (let generation = 1; generation <= 12; generation++) {
    const started = await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) });
    assert.equal(started.status, 200);
    const match = started.body.events.filter((event) => event.kind === 'game_started').at(-1).payload;
    const ticket = Buffer.from(match.relay.ticket, 'base64url');
    assert.equal(ticket[0], 2); assert.equal(ticket.length, 98);
    assert.equal(ticket.readBigUInt64LE(58), BigInt(generation));
    assert.equal(ticket.readBigUInt64LE(18), 100120n);
    assert.notEqual(match.match_id, previousMatch); assert.notEqual(match.relay.payload_key, previousKey);
    previousMatch = match.match_id; previousKey = match.relay.payload_key;
    assert.equal(f.store.get(`match:${match.match_id}`).payload_key, match.relay.payload_key);
    const cancelled = await f.command(host, 'finish', { match_id: match.match_id });
    assert.equal(cancelled.status, 200); assert.equal(f.store.list('match:').length, 0);
    assert.equal((await f.command(host, 'renew_relay', { match_id: match.match_id })).status, 409);
    revision = cancelled.body.events.filter((event) => event.kind === 'options').at(-1).payload.revision;
    f.restart();
    for (const user of [host, guest]) {
      await f.command(user, 'map_status', { status: 1, ...agreement(revision) });
      await f.command(user, 'ready', { ready: true, ...agreement(revision) });
    }
  }
  assert.equal((await f.command(host, 'start', { users: ['Host', 'Guest'], ...agreement(revision) })).status, 429);
  assert.equal(f.store.list('account:').find((account) => account.player_id === host.player_id).admission_generation, 12);
});

test('canonical schemas reject fanout padding and state updates stay bounded', async () => {
  const f = fixture(), host = await f.account('Host'), guest = await f.account('Guest');
  await room(f, host, guest);
  assert.equal((await f.command(guest, 'choices', { row: 1, slot: { unused: 'x'.repeat(60000) }, observer: false })).status, 400);
  assert.equal((await f.command(guest, 'choices', { row: 1, slot, observer: false })).status, 200);
  for (const changed of [
    { ...options, unused: 'x'.repeat(60000) },
    { ...options, launch: { ...options.launch, selected_map_file: '../outside.map' } },
    { ...options, launch: { ...options.launch, humans: [human('Host'), human('Guest')] } },
  ]) assert.equal((await f.command(host, 'options', { options: changed })).status, 400);
  for (let i = 0; i < 40; i++)
    assert.equal((await f.command(host, 'options', { options: { ...options, seed: i } })).status, 200);
  const guestId = f.store.list('session:').find((session) => session.player_id === guest.player_id).id;
  const retained = f.store.events(guestId, 0);
  assert.equal(retained.filter((event) => event.kind === 'options').length, 1);
  await f.command(host, 'leave'); await room(f, host, guest);
  assert.equal(f.store.events(guestId, 0).filter((event) => event.kind === 'options').length, 2,
    'replaceable state is coalesced only within the same room');
  assert.throws(() => f.emit(guest, 'error', { message: 'x'.repeat(65536) }), /too large/);
  for (let i = 0; i < 50; i++) f.emit(guest, 'error', { message: 'x'.repeat(50000) });
  const current = f.store.get(`session:${guestId}`);
  assert.ok(current.critical_floor > 0, 'only actual critical history eviction raises the floor');
  const reply = await f.request(`/v1/events?after=${current.critical_floor}`, guest.token);
  assert.equal(reply.status, 200); assert.ok(Buffer.byteLength(JSON.stringify(reply.body)) <= 256 * 1024);
});

test('public chat rejects escaped lone surrogates without poisoning recipient polling', async () => {
  const f = fixture(), sender = await f.account('Sender'), reader = await f.account('Reader');
  for (const user of [sender, reader]) await f.command(user, 'join', { channel: 'Lob_41_0', key: '' });
  const before = await f.request('/v1/events?after=0', reader.token);
  for (const text of ['\ud800', '\udfff'])
    assert.equal((await f.command(sender, 'chat', { text, to: [] })).status, 400);
  const clean = await f.request(`/v1/events?after=${before.body.cursor}`, reader.token);
  assert.equal(clean.status, 200); assert.deepEqual(clean.body.events, []);
  assert.equal(clean.body.cursor, before.body.cursor);
  assert.equal((await f.command(sender, 'chat', { text: 'Hello \ud83d\ude00', to: [] })).status, 200);
  const delivered = await f.request(`/v1/events?after=${clean.body.cursor}`, reader.token);
  assert.equal(delivered.status, 200);
  assert.equal(delivered.body.events.find((event) => event.kind === 'chat').payload.text, 'Hello \ud83d\ude00');
});

test('JSON ingress validates nested keys and bounds container depth without recursion', async () => {
  const f = fixture();
  await assert.rejects(f.parse({ nested: [{ ['\ud800']: 'valid value' }] }), { status: 400 });
  const nested = (depth) => {
    let value = 'leaf';
    for (let index = 0; index < depth; index++) value = index % 2 && index < depth - 1 ? [value] : { child: value };
    return value;
  };
  assert.deepEqual(await f.parse(nested(64)), nested(64));
  await assert.rejects(f.parse(nested(65)), { status: 400 });
});
