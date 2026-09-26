"""Fill the empty white panel of a Higgsfield tafsir flyer with the WhatsApp QR code.

usage: uv run --with "qrcode[pil]" --with opencv-python-headless python add_qr.py in.png out.png [link]
"""
import sys
import cv2
import qrcode
from PIL import Image

LINK = sys.argv[3] if len(sys.argv) > 3 else "https://chat.whatsapp.com/H7Hqry1hS3eKrjMRO9O7rK"
S = 3  # ponytail: plain Lanczos 3x so the QR prints sharp; AI-upscale if the artwork needs more detail

src, dst = sys.argv[1], sys.argv[2]
im = Image.open(src).convert("RGB")
w, h = im.size
# the panel = largest near-square blob of pure white in the lower half
arr = cv2.cvtColor(cv2.imread(src), cv2.COLOR_BGR2GRAY)
mask = ((arr > 248) * 255).astype("uint8")
mask[: h // 2] = 0
n, _, stats, _ = cv2.connectedComponentsWithStats(mask)
x0, y0, bw, bh = max(
    (st[:4] for st in stats[1:] if 0.8 < st[2] / st[3] < 1.25 and st[4] > 0.8 * st[2] * st[3]),
    key=lambda st: st[2] * st[3],
)
pad = 4
x0, y0, x1, y1 = x0 + pad, y0 + pad, x0 + bw - pad, y0 + bh - pad
print("panel", x0, y0, x1, y1)

big = im.resize((w * S, h * S), Image.LANCZOS)
side = min(x1 - x0, y1 - y0) * S
q = qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_M, border=2, box_size=10)
q.add_data(LINK)
q.make(fit=True)
qi = q.make_image(fill_color=(15, 25, 30), back_color="white").convert("RGB").resize((side, side), Image.NEAREST)
big.paste(qi, ((x0 + x1) * S // 2 - side // 2, (y0 + y1) * S // 2 - side // 2))
big.save(dst)

decoded = cv2.QRCodeDetector().detectAndDecode(cv2.imread(dst))[0]
assert decoded == LINK, f"QR check failed: {decoded!r}"
print("ok", dst)
