import os
import sys

from PIL import Image

d, tag, cols = sys.argv[1], sys.argv[2], int(sys.argv[3])
scale = float(sys.argv[4]) if len(sys.argv) > 4 else 0.36
ims = []
while os.path.exists(f'{d}/{tag}_{len(ims)}.png'):
    ims.append(Image.open(f'{d}/{tag}_{len(ims)}.png').convert('RGB'))
if not ims:
    sys.exit(f'no frames {d}/{tag}_0.png')
w, h = ims[0].size
tw, th = int(w * scale), int(h * scale)
rows = (len(ims) + cols - 1) // cols
sheet = Image.new('RGB', (tw * cols, th * rows), 'white')
for i, im in enumerate(ims):
    sheet.paste(im.resize((tw, th)), ((i % cols) * tw, (i // cols) * th))
sheet.save(f'{d}/{tag}_sheet.png')
print(f'{d}/{tag}_sheet.png', sheet.size)
