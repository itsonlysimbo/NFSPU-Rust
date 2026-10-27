"""Record a retail reference trace for the behaviour oracle (docs/ORACLE.md).

Run against the COPY of the install in the retail copy (C:/Users/CB/Downloads/NFS5 - Oracle), with the game already inside a
race (see docs/provenance/oracle-capture.md for the menu clicks). The tool opens the game process
read-only (PROCESS_VM_READ), never writes its memory, and only injects keyboard input.

  python scripts/oracle_capture.py <name> <ticks> <sample_every> "<from_tick:throttle:brake:steer>" ...

The script uses the same "from_tick:throttle:brake:steer" lines as oracle/cases/*.toml; tick 0 is
the GO tick of the race (448 in the game's own 64 Hz counter). Throttle and brake are on/off
(keyboard), steer is -1, 0 or 1. Output: oracle/ref/<name>.csv in the runner's format
(tick, speed km/h, rpm, gear, x, z, yaw) with x/z in the start frame (z forward is negative,
like our sim), plus logs/oracle_capture_<name>_raw.csv with every tick seen (world position).
"""
import math, os, struct, sys, time

sys.path.insert(0, os.path.dirname(__file__))
from oracle_capture_lib import SC, Proc, client_size, find_window, key, pid_of, tap, click, u32  # noqa: E402

PLAYER_IDX = 0x65741C   # which entry of the car table is the player
CAR_TABLE = 0x5E8E2C    # car pointers
GO_TICK = 448           # tick at which the race clock starts (measured, matches our GO_TICK)
OFF_TICK, OFF_SPEED, OFF_RPM, OFF_GEAR = 0x88, 0xEC4, 0xDB4, 0xD82
OFF_POS, OFF_FWD = 0x7F8, 0x3D0  # 0x7f8: live body position; 0x3d0: heading, only refreshed now and then (used at standstill)


def read_state(p, car):
    b = p.read(car, 0xEE0)
    if b is None or len(b) < 0xEE0:
        return None
    tick = struct.unpack_from("<i", b, OFF_TICK)[0]
    speed = struct.unpack_from("<f", b, OFF_SPEED)[0]
    rpm = struct.unpack_from("<f", b, OFF_RPM)[0]
    gear = b[OFF_GEAR]
    pos = struct.unpack_from("<3f", b, OFF_POS)
    fwd = struct.unpack_from("<3f", b, OFF_FWD)
    return tick, speed, rpm, gear, pos, fwd


def parse_script(lines):
    out = []
    for l in lines:
        t, th, br, st = l.split(":")
        out.append((int(t), float(th), float(br), float(st)))
    return sorted(out)


def apply(held, th, br, st):
    want = {SC["up"]: th > 0.5, SC["down"]: br > 0.5, SC["left"]: st < -0.5, SC["right"]: st > 0.5}
    for sc, on in want.items():
        if on != held.get(sc, False):
            key(sc, on)
            held[sc] = on


def main():
    name, ticks, every = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    script = parse_script(sys.argv[4:])
    h = find_window()
    p = Proc(pid_of(h))
    u32.SetForegroundWindow(h)
    time.sleep(0.5)
    # restart the race from the pause menu so tick 0 is the load of a fresh race
    tap(SC["esc"])
    time.sleep(1.0)
    cw, ch = client_size(h)
    click(h, cw // 2, int(ch * 533 / 1080))  # "race again" (pause menu is centred)
    time.sleep(0.5)
    car = None
    last_tick = 10**9
    t_end = time.time() + 90
    held, rows, si = {}, [], 0
    start_pos = None
    while time.time() < t_end:
        idx = p.u32(PLAYER_IDX)
        car = p.u32(CAR_TABLE + 4 * idx) if idx is not None and idx < 8 else None
        if not car:
            continue
        s = read_state(p, car)
        if s is None or s[0] == last_tick:
            continue
        tick = s[0]
        if tick < last_tick - 200 and tick > 2000000:
            continue
        last_tick = tick
        rel = tick - GO_TICK
        if rel < 0:
            start_pos = (s[4], s[5])  # position and heading while waiting at the grid
            continue
        while si < len(script) and script[si][0] <= rel:
            apply(held, *script[si][1:])
            si += 1
        rows.append((rel,) + s[1:])
        if rel >= ticks:
            break
    apply(held, 0, 0, 0)
    if start_pos is None or not rows:
        sys.exit("no samples: is the game inside a race?")
    (x0, y0, z0), (fx, fy, fz) = start_pos
    n = math.hypot(fx, fz)
    fx, fz = fx / n, fz / n
    yaw0 = math.atan2(fx, -fz)  # stale snapshot: replaced below by the direction of the first launch
    os.makedirs("logs", exist_ok=True)
    with open(f"logs/oracle_capture_{name}_raw.csv", "w") as f:
        f.write("tick,speed_ms,rpm,gear,x,y,z,fwd_x,fwd_y,fwd_z\n")
        for r in rows:
            f.write("%d,%.5f,%.3f,%d,%.4f,%.4f,%.4f,%.5f,%.5f,%.5f\n" % ((r[0], r[1], r[2], r[3]) + r[4] + r[5]))
    # resample onto every `every` ticks (linear between the two nearest ticks seen)
    by = {r[0]: r for r in rows}
    keys_ = sorted(by)

    def sample(t):
        if t in by:
            return by[t]
        lo = max((k for k in keys_ if k < t), default=None)
        hi = min((k for k in keys_ if k > t), default=None)
        if lo is None or hi is None:
            return None
        a, b, w = by[lo], by[hi], (t - lo) / (hi - lo)
        mix = lambda u, v: u + (v - u) * w
        return (t, mix(a[1], b[1]), mix(a[2], b[2]), a[3], tuple(mix(u, v) for u, v in zip(a[4], b[4])),
                tuple(mix(u, v) for u, v in zip(a[5], b[5])))

    if not any(abs(sc[3]) > 0.5 and sc[0] < 192 for sc in script):
        a0, a1 = sample(0), sample(192)
        if a0 and a1 and math.hypot(a1[4][0] - a0[4][0], a1[4][2] - a0[4][2]) > 5:
            ddx, ddz = a1[4][0] - a0[4][0], a1[4][2] - a0[4][2]
            n2 = math.hypot(ddx, ddz)
            fx, fz = ddx / n2, ddz / n2
            yaw0 = math.atan2(fx, -fz)
            x0, z0 = a0[4][0], a0[4][2]

    def course(t):
        """Heading as the direction of travel over the last 16 ticks (start heading until it moves 0.5 m)."""
        r = sample(t)
        q = sample(max(t - 16, 0))
        if r is None or q is None:
            return yaw0
        dx, dz = r[4][0] - q[4][0], r[4][2] - q[4][2]
        if math.hypot(dx, dz) < 0.5 or r[1] < 1.0:
            return yaw0
        return math.atan2(dx, -dz)

    os.makedirs("oracle/ref", exist_ok=True)
    with open(f"oracle/ref/{name}.csv", "w") as f:
        f.write(f"# {name} trace from RETAIL (nfs5.exe copy, memory read-only, scripts/oracle_capture.py); x,z in the start frame\n")
        f.write("tick,speed_kmh,rpm,gear,x,z,yaw\n")
        for t in range(0, ticks + 1, every):
            r = sample(t)
            if r is None:
                continue
            dx, dz = r[4][0] - x0, r[4][2] - z0
            fwd_d = dx * fx + dz * fz           # metres along the start heading
            lat = dx * -fz + dz * fx            # metres to the left of the start heading (retail frame)
            fwd2 = course(r[0])
            f.write("%d,%.4f,%.2f,%d,%.4f,%.4f,%.5f\n" % (t, r[1] * 3.6, r[2], r[3], -lat, -fwd_d, (fwd2 - yaw0 + math.pi) % (2 * math.pi) - math.pi))
    print("wrote oracle/ref/%s.csv from %d ticks" % (name, len(rows)))


if __name__ == "__main__":
    main()
