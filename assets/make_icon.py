"""Genera assets/app.ico a partir de un dibujo vectorial simple.

Uso: python assets/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

SIZE = 1024
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

BG_TOP = (43, 58, 70, 255)
BG_BOTTOM = (17, 24, 32, 255)
OUTLINE = (245, 247, 250, 255)
FILL = (63, 185, 80, 255)


def background() -> Image.Image:
    gradient = Image.new("RGBA", (SIZE, SIZE))
    draw = ImageDraw.Draw(gradient)
    for y in range(SIZE):
        t = y / (SIZE - 1)
        color = tuple(
            round(BG_TOP[i] + (BG_BOTTOM[i] - BG_TOP[i]) * t) for i in range(3)
        ) + (255,)
        draw.line([(0, y), (SIZE, y)], fill=color)

    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [24, 24, SIZE - 24, SIZE - 24], radius=210, fill=255
    )
    result = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    result.paste(gradient, (0, 0), mask)
    return result


def draw_icon() -> Image.Image:
    image = background()
    draw = ImageDraw.Draw(image)

    body = [258, 336, 706, 688]
    draw.rounded_rectangle(body, radius=48, outline=OUTLINE, width=34)
    draw.rounded_rectangle([704, 458, 768, 566], radius=18, fill=OUTLINE)

    inner = [310, 388, 654, 636]
    draw.rounded_rectangle(inner, radius=18, fill=FILL)
    return image


def main() -> None:
    icon = draw_icon()
    target = Path(__file__).with_name("app.ico")
    frames = [icon.resize((size, size), Image.LANCZOS) for size in SIZES]
    frames[-1].save(target, format="ICO", sizes=[(size, size) for size in SIZES])
    print(f"escrito {target}")


if __name__ == "__main__":
    main()
