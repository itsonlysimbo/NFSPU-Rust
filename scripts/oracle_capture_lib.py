"""Helpers for the retail oracle capture: find the game window, read its memory (read-only),
send keyboard/mouse input and take screenshots. Only ever run against the copy in
the retail copy (C:/Users/CB/Downloads/NFS5 - Oracle) (docs/provenance/oracle-capture.md)."""
import ctypes, ctypes.wintypes as w, struct, time, zlib

k32 = ctypes.windll.kernel32
u32 = ctypes.windll.user32
g32 = ctypes.windll.gdi32
u32.SetProcessDPIAware()
k32.OpenProcess.restype = w.HANDLE
k32.ReadProcessMemory.argtypes = [w.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
                                  ctypes.POINTER(ctypes.c_size_t)]


def find_window(title="Porsche"):
    res = []

    @ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    def cb(h, _):
        if u32.IsWindowVisible(h):
            buf = ctypes.create_unicode_buffer(256)
            u32.GetWindowTextW(h, buf, 256)
            if buf.value == title:
                res.append(h)
        return True

    u32.EnumWindows(cb, 0)
    global GAME_HWND
    GAME_HWND = res[0] if res else None
    return GAME_HWND


def pid_of(hwnd):
    p = w.DWORD()
    u32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
    return p.value


class Proc:
    """Read-only handle: PROCESS_VM_READ | PROCESS_QUERY_INFORMATION. Never writes."""

    def __init__(self, pid):
        self.h = k32.OpenProcess(0x0010 | 0x0400, False, pid)
        if not self.h:
            raise OSError("OpenProcess failed")

    def read(self, addr, n):
        buf = ctypes.create_string_buffer(n)
        got = ctypes.c_size_t()
        if not k32.ReadProcessMemory(self.h, addr, buf, n, ctypes.byref(got)):
            return None
        return buf.raw[:got.value]

    def u32(self, a):
        b = self.read(a, 4)
        return struct.unpack("<I", b)[0] if b and len(b) == 4 else None

    def f32(self, a):
        b = self.read(a, 4)
        return struct.unpack("<f", b)[0] if b and len(b) == 4 else None


def client_origin(hwnd):
    pt = w.POINT(0, 0)
    u32.ClientToScreen(hwnd, ctypes.byref(pt))
    return pt.x, pt.y


def client_size(hwnd):
    r = w.RECT()
    u32.GetClientRect(hwnd, ctypes.byref(r))
    return r.right, r.bottom


class _KI(ctypes.Structure):
    _fields_ = [("wVk", w.WORD), ("wScan", w.WORD), ("dwFlags", w.DWORD), ("time", w.DWORD),
                ("dwExtraInfo", ctypes.c_size_t)]


class _MI(ctypes.Structure):
    _fields_ = [("dx", w.LONG), ("dy", w.LONG), ("mouseData", w.DWORD), ("dwFlags", w.DWORD),
                ("time", w.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _U(ctypes.Union):
    _fields_ = [("ki", _KI), ("mi", _MI)]


class _IN(ctypes.Structure):
    _fields_ = [("type", w.DWORD), ("u", _U)]


GAME_HWND = None  # set by find_window(); input is refused unless the game is the foreground window


def _guard():
    if GAME_HWND is not None and u32.GetForegroundWindow() != GAME_HWND:
        raise RuntimeError("the game is not the foreground window: refusing to send input")


def key(scan, down):
    """Scan-code key event (DirectInput reads scan codes). Extended keys: pass scan | 0x100."""
    _guard()
    flags = 0x0008 | (0 if down else 0x0002) | (0x0001 if scan & 0x100 else 0)
    i = _IN(1, _U(ki=_KI(0, scan & 0xff, flags, 0, 0)))
    u32.SendInput(1, ctypes.byref(i), ctypes.sizeof(i))


def tap(scan, hold=0.08):
    key(scan, True)
    time.sleep(hold)
    key(scan, False)
    time.sleep(0.08)


def click(hwnd, x, y):
    _guard()
    ox, oy = client_origin(hwnd)
    u32.SetCursorPos(ox + x, oy + y)
    time.sleep(0.15)
    for f in (0x0002, 0x0004):
        i = _IN(0, _U(mi=_MI(0, 0, 0, f, 0, 0)))
        u32.SendInput(1, ctypes.byref(i), ctypes.sizeof(i))
        time.sleep(0.08)


class _BIH(ctypes.Structure):
    _fields_ = [("s", w.DWORD), ("w", w.LONG), ("h", w.LONG), ("p", w.WORD), ("b", w.WORD),
                ("c", w.DWORD), ("si", w.DWORD), ("x", w.LONG), ("y", w.LONG), ("u", w.DWORD),
                ("i", w.DWORD)]


def screenshot(hwnd, path):
    """Screen grab of the client area as a PNG (BitBlt from the desktop, no extra packages)."""
    ox, oy = client_origin(hwnd)
    cw, ch = client_size(hwnd)
    sdc = u32.GetDC(0)
    mdc = g32.CreateCompatibleDC(sdc)
    bmp = g32.CreateCompatibleBitmap(sdc, cw, ch)
    g32.SelectObject(mdc, bmp)
    g32.BitBlt(mdc, 0, 0, cw, ch, sdc, ox, oy, 0x00CC0020)
    bi = _BIH(ctypes.sizeof(_BIH), cw, -ch, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(cw * ch * 4)
    g32.GetDIBits(mdc, bmp, 0, ch, buf, ctypes.byref(bi), 0)
    d = buf.raw
    raw = bytearray()
    for y in range(ch):
        raw.append(0)
        row = d[y * cw * 4:(y + 1) * cw * 4]
        rgb = bytearray(cw * 3)
        rgb[0::3] = row[2::4]
        rgb[1::3] = row[1::4]
        rgb[2::3] = row[0::4]
        raw += rgb

    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xffffffff)

    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", cw, ch, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(bytes(raw), 6)) + chunk(b"IEND", b""))
    open(path, "wb").write(png)
    g32.DeleteObject(bmp)
    g32.DeleteDC(mdc)
    u32.ReleaseDC(0, sdc)


SC = dict(enter=0x1c, esc=0x01, up=0x148, down=0x150, left=0x14b, right=0x14d, space=0x39,
          a=0x1e, z=0x2c, s=0x1f, x=0x2d)
