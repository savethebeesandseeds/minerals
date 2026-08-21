import unittest

from build_map_demo_asset import polygon_parts


class PolygonPartsTests(unittest.TestCase):
    def test_polygon_is_one_part_with_its_holes(self) -> None:
        outer = [[0.0, 0.0], [2.0, 0.0], [0.0, 2.0], [0.0, 0.0]]
        hole = [[0.2, 0.2], [0.4, 0.2], [0.2, 0.4], [0.2, 0.2]]

        parts = list(polygon_parts({"type": "Polygon", "coordinates": [outer, hole]}))

        self.assertEqual(parts, [[outer, hole]])

    def test_multipolygon_keeps_each_outer_ring_as_land(self) -> None:
        first = [[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 0.0]]]
        second = [[[3.0, 3.0], [4.0, 3.0], [3.0, 4.0], [3.0, 3.0]]]

        parts = list(
            polygon_parts({"type": "MultiPolygon", "coordinates": [first, second]})
        )

        self.assertEqual(parts, [first, second])

    def test_unsupported_geometry_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            list(polygon_parts({"type": "LineString", "coordinates": []}))


if __name__ == "__main__":
    unittest.main()
