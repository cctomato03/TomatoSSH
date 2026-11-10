from PIL import Image, ImageDraw

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 圆角背景
pad = 48
radius = 200
bg_colors = [(22, 23, 29), (36, 20, 24)]
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
for y in range(S):
    t = y / S
    r = int(bg_colors[0][0] * (1 - t) + bg_colors[1][0] * t)
    g = int(bg_colors[0][1] * (1 - t) + bg_colors[1][1] * t)
    b = int(bg_colors[0][2] * (1 - t) + bg_colors[1][2] * t)
    gd.line([(0, y), (S, y)], fill=(r, g, b, 255))

mask = Image.new("L", (S, S), 0)
md = ImageDraw.Draw(mask)
md.rounded_rectangle([pad, pad, S - pad, S - pad], radius=radius, fill=255)
img.paste(grad, (0, 0), mask)
d = ImageDraw.Draw(img)

# 番茄主体
cx, cy, r = 512, 588, 296
d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(240, 76, 60, 255))
# 高光
d.ellipse([cx - r + 60, cy - r + 54, cx - r + 210, cy - r + 150], fill=(255, 122, 101, 190))
# 阴影
d.ellipse([cx - 70, cy + r - 90, cx + 130, cy + r - 10], fill=(200, 44, 32, 90))

# 叶子
d.polygon([(cx, cy - r - 10), (cx + 150, cy - r - 96), (cx + 40, cy - r + 46)], fill=(62, 207, 142, 255))
d.polygon([(cx, cy - r - 10), (cx - 150, cy - r - 96), (cx - 40, cy - r + 46)], fill=(52, 186, 126, 255))
d.polygon([(cx, cy - r - 24), (cx + 26, cy - r - 150), (cx - 26, cy - r - 150)], fill=(96, 220, 160, 255))

# 终端提示符 ">" 与 "_"
w = 46
d.line([(410, 486), (536, 588), (410, 690)], fill=(255, 255, 255, 255), width=w, joint="curve")
d.line([(574, 672), (700, 672)], fill=(255, 255, 255, 255), width=w)

img.save(r"C:\Users\Apple\IdeaProjects\TomatoSSH\src-tauri\icon-source.png")
print("ok")
