import sys


# Pixels in X0 Y0 X1 Y1 that differ from a reference PPM by more than 24 in any channel.
def load(path):
    with open(path, "rb") as file:
        data = file.read()
    header = data.split(b"\n", 3)
    w, _ = map(int, header[1].split())
    return w, header[3]


w, a = load(sys.argv[1])
_, b = load(sys.argv[2])
x0, y0, x1, y1 = (int(v) for v in sys.argv[3:7])
n = 0
for y in range(y0, y1):
    row = y * w * 3
    for x in range(x0, x1):
        i = row + 3 * x
        if (
            abs(a[i] - b[i]) > 24
            or abs(a[i + 1] - b[i + 1]) > 24
            or abs(a[i + 2] - b[i + 2]) > 24
        ):
            n += 1
print(n)
