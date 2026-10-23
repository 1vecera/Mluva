import sys

# Count ink pixels (Mluva's red, 255/59/48 with antialiasing) in a binary PPM, optionally in X0 Y0 X1 Y1.
with open(sys.argv[1], "rb") as file:
    data = file.read()
header = data.split(b"\n", 3)
w, h = map(int, header[1].split())
px = header[3]
x0, y0, x1, y1 = (int(v) for v in sys.argv[2:6]) if len(sys.argv) > 5 else (0, 0, w, h)
n = 0
for y in range(y0, y1):
    row = y * w * 3
    for x in range(x0, x1):
        i = row + 3 * x
        if px[i] > 200 and px[i + 1] < 110 and px[i + 2] < 100:
            n += 1
print(n)
