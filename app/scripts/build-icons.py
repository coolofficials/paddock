import json
import math
import re
from pathlib import Path
import xml.etree.ElementTree as xml

ROOT = Path(__file__).resolve().parent.parent
NAMES = ['check', 'chevron-down', 'circle', 'arrow-right', 'arrow-up-right', 'layout-grid', 'sliders-horizontal', 'monitor', 'undo-2', 'search', 'star', 'play', 'square']


def arc_points(start, end, radius_x, radius_y, rotation, large, sweep):
    radius_x, radius_y = abs(radius_x), abs(radius_y)
    if not radius_x or not radius_y or start == end:
        return [end]
    angle = math.radians(rotation)
    cosine, sine = math.cos(angle), math.sin(angle)
    difference = ((start[0]-end[0])/2, (start[1]-end[1])/2)
    local_x = cosine*difference[0]+sine*difference[1]
    local_y = -sine*difference[0]+cosine*difference[1]
    scale = max(1, math.sqrt(local_x**2/radius_x**2+local_y**2/radius_y**2))
    radius_x *= scale
    radius_y *= scale
    denominator = radius_x**2*local_y**2+radius_y**2*local_x**2
    factor = math.sqrt(max(0, (radius_x**2*radius_y**2-denominator)/denominator)) * (-1 if bool(large) == bool(sweep) else 1)
    center_x = factor*radius_x*local_y/radius_y
    center_y = -factor*radius_y*local_x/radius_x
    center = (cosine*center_x-sine*center_y+(start[0]+end[0])/2, sine*center_x+cosine*center_y+(start[1]+end[1])/2)
    begin = math.atan2((local_y-center_y)/radius_y, (local_x-center_x)/radius_x)
    finish = math.atan2((-local_y-center_y)/radius_y, (-local_x-center_x)/radius_x)
    delta = (finish-begin) % (2*math.pi)
    if not sweep:
        delta -= 2*math.pi
    count = max(4, math.ceil(abs(delta)*16))
    return [(center[0]+cosine*radius_x*math.cos(begin+delta*step/count)-sine*radius_y*math.sin(begin+delta*step/count), center[1]+sine*radius_x*math.cos(begin+delta*step/count)+cosine*radius_y*math.sin(begin+delta*step/count)) for step in range(1, count+1)]


def parse_path(value):
    tokens = re.findall(r'[a-zA-Z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?', value)
    position, point, origin, command = 0, (0, 0), (0, 0), None
    strokes, stroke = [], []
    sizes = {'M': 2, 'L': 2, 'H': 1, 'V': 1, 'A': 7, 'C': 6, 'Q': 4}
    while position < len(tokens):
        if tokens[position].isalpha():
            command = tokens[position]
            position += 1
        kind = command.upper()
        if kind == 'Z':
            stroke.append(origin)
            point = origin
            command = None
            continue
        count = sizes[kind]
        values = list(map(float, tokens[position:position+count]))
        position += count
        relative = command.islower()
        def endpoint(offset):
            return (values[offset]+(point[0] if relative else 0), values[offset+1]+(point[1] if relative else 0))
        if kind == 'M':
            if stroke:
                strokes.append(stroke)
            point = endpoint(0)
            origin = point
            stroke = [point]
            command = 'l' if relative else 'L'
        elif kind in ['L', 'H', 'V']:
            point = endpoint(0) if kind == 'L' else (values[0]+(point[0] if relative else 0), point[1]) if kind == 'H' else (point[0], values[0]+(point[1] if relative else 0))
            stroke.append(point)
        elif kind == 'A':
            end = endpoint(5)
            stroke.extend(arc_points(point, end, *values[:5]))
            point = end
        else:
            controls = [point]+[endpoint(offset) for offset in range(0, count, 2)]
            for step in range(1, 25):
                fraction = step/24
                interpolated = controls[:]
                while len(interpolated) > 1:
                    interpolated = [tuple(first[axis]*(1-fraction)+second[axis]*fraction for axis in (0, 1)) for first, second in zip(interpolated, interpolated[1:])]
                stroke.append(interpolated[0])
            point = controls[-1]
    if stroke:
        strokes.append(stroke)
    return strokes


def read_icon(name):
    strokes = []
    for element in xml.parse(ROOT/'assets/framework/icons'/f'{name}.svg').getroot():
        tag = element.tag.split('}')[-1]
        attributes = element.attrib
        number = lambda key, default=0: float(attributes.get(key, default))
        if tag == 'path':
            strokes.extend(parse_path(attributes['d']))
        elif tag == 'line':
            strokes.append([(number('x1'), number('y1')), (number('x2'), number('y2'))])
        elif tag == 'circle':
            strokes.append([(number('cx')+number('r')*math.cos(step*math.pi/32), number('cy')+number('r')*math.sin(step*math.pi/32)) for step in range(65)])
        elif tag in ['polygon', 'polyline']:
            numbers = list(map(float, re.findall(r'[-+]?[\d.]+', attributes['points'])))
            points = list(zip(numbers[::2], numbers[1::2]))
            strokes.append(points+[points[0]] if tag == 'polygon' else points)
        elif tag == 'rect':
            left, top, width, height, radius = number('x'), number('y'), number('width'), number('height'), number('rx')
            if radius:
                points = []
                for center_x, center_y, begin in [(left+width-radius, top+radius, -90), (left+width-radius, top+height-radius, 0), (left+radius, top+height-radius, 90), (left+radius, top+radius, 180)]:
                    points.extend((center_x+radius*math.cos(math.radians(begin+step*90/12)), center_y+radius*math.sin(math.radians(begin+step*90/12))) for step in range(13))
                strokes.append(points+[points[0]])
            else:
                strokes.append([(left, top), (left+width, top), (left+width, top+height), (left, top+height), (left, top)])
        else:
            raise ValueError(tag)
    return [[[round(coordinate, 5) for coordinate in point] for point in stroke] for stroke in strokes]


(ROOT/'assets/framework/lucide-paths.json').write_text(json.dumps({name: read_icon(name) for name in NAMES}, separators=(',', ':')), encoding='utf-8')
