from PIL import Image, ImageDraw
import os

# Create a simple crab icon
size = 256
img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# Simple crab shape - red circle with claws
draw.ellipse([50, 80, 206, 176], fill=(233, 69, 96, 255))
draw.ellipse([30, 60, 80, 110], fill=(233, 69, 96, 255))
draw.ellipse([176, 60, 226, 110], fill=(233, 69, 96, 255))
draw.text([100, 105], "🦀", fill=(255, 255, 255, 255))

# Ensure icons directory exists
os.makedirs('icons', exist_ok=True)

# Save as PNG (can be converted by tauri)
img.save('icons/icon.png')
print("Created icons/icon.png")
