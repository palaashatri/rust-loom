from pathlib import Path
from zipfile import ZipFile
from xml.etree import ElementTree as ET

archive = Path(__file__).with_name("chart.xlsx")
with ZipFile(archive) as workbook:
    for name in ("xl/worksheets/sheet1.xml", "xl/charts/chart1.xml"):
        try:
            ET.fromstring(workbook.read(name))
            print(name, "OK")
        except ET.ParseError as error:
            print(name, error)
