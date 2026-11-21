from pathlib import Path
import math
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent
scale = 4
image = Image.new('RGBA', (256 * scale, 256 * scale))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((8 * scale, 8 * scale, 248 * scale, 248 * scale), radius=54 * scale, fill='#182d25')
draw.line([(68 * scale, 210 * scale), (68 * scale, 63 * scale)], fill='#b6e49e', width=12 * scale)
svg = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">', '<rect x="8" y="8" width="240" height="240" rx="54" fill="#182d25"/>', '<path d="M68 210V63" stroke="#b6e49e" stroke-width="12" stroke-linecap="round"/>']
for row in range(3):
    for column in range(4):
        points = []
        for step in range(9):
            horizontal = (column + step / 8) / 4
            points.append((74 + 134 * horizontal, 69 + 12 * math.sin(horizontal * math.tau) + row * 28))
        for step in range(8, -1, -1):
            horizontal = (column + step / 8) / 4
            points.append((74 + 134 * horizontal, 69 + 12 * math.sin(horizontal * math.tau) + (row + 1) * 28))
        color = '#f0f5ed' if (row + column) % 2 == 0 else '#507b5c'
        draw.polygon([(round(horizontal * scale), round(vertical * scale)) for horizontal, vertical in points], fill=color)
        coordinates = ' '.join(f'{horizontal:.2f},{vertical:.2f}' for horizontal, vertical in points)
        svg.append(f'<polygon points="{coordinates}" fill="{color}"/>')
draw.line([(111 * scale, 197 * scale), (199 * scale, 197 * scale)], fill='#b6e49e', width=8 * scale)
draw.line([(144 * scale, 215 * scale), (199 * scale, 215 * scale)], fill='#507b5c', width=7 * scale)
svg.extend(['<path d="M111 197H199" stroke="#b6e49e" stroke-width="8"/>', '<path d="M144 215H199" stroke="#507b5c" stroke-width="7"/>', '</svg>'])
(root / 'assets/app.svg').write_text('\n'.join(svg), encoding='utf-8')
image = image.resize((256, 256), Image.Resampling.LANCZOS)
image.save(root / 'assets/app.ico', sizes=[(16,16), (24,24), (32,32), (48,48), (64,64), (128,128), (256,256)])
image.save(root / 'outputs/launcher-icon.png')
