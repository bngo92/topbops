use topbops_wasm::tournament::Node;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn test_generate_tournament() {
    for (input, output) in [
        (2, vec![Some((1, false)), Some((0, true)), Some((2, false))]),
        (
            3,
            vec![
                None,
                Some((1, false)),
                None,
                Some((0, true)),
                Some((3, false)),
                Some((0, true)),
                Some((2, false)),
            ],
        ),
        (
            4,
            vec![
                Some((1, false)),
                Some((0, true)),
                Some((4, false)),
                Some((0, true)),
                Some((3, false)),
                Some((0, true)),
                Some((2, false)),
            ],
        ),
        (
            5,
            vec![
                None,
                Some((1, false)),
                None,
                Some((0, true)),
                Some((5, false)),
                Some((0, true)),
                Some((4, false)),
                Some((0, true)),
                None,
                Some((3, false)),
                None,
                Some((0, true)),
                None,
                Some((2, false)),
                None,
            ],
        ),
        (
            7,
            vec![
                None,
                Some((1, false)),
                None,
                Some((0, true)),
                Some((5, false)),
                Some((0, true)),
                Some((4, false)),
                Some((0, true)),
                Some((3, false)),
                Some((0, true)),
                Some((6, false)),
                Some((0, true)),
                Some((7, false)),
                Some((0, true)),
                Some((2, false)),
            ],
        ),
        (
            8,
            vec![
                Some((1, false)),
                Some((0, true)),
                Some((8, false)),
                Some((0, true)),
                Some((5, false)),
                Some((0, true)),
                Some((4, false)),
                Some((0, true)),
                Some((3, false)),
                Some((0, true)),
                Some((6, false)),
                Some((0, true)),
                Some((7, false)),
                Some((0, true)),
                Some((2, false)),
            ],
        ),
        (
            12,
            vec![
                None,
                Some((1, false)),
                None,
                Some((0, true)),
                Some((9, false)),
                Some((0, true)),
                Some((8, false)),
                Some((0, true)),
                Some((5, false)),
                Some((0, true)),
                Some((12, false)),
                Some((0, true)),
                None,
                Some((4, false)),
                None,
                Some((0, true)),
                None,
                Some((3, false)),
                None,
                Some((0, true)),
                Some((11, false)),
                Some((0, true)),
                Some((6, false)),
                Some((0, true)),
                Some((7, false)),
                Some((0, true)),
                Some((10, false)),
                Some((0, true)),
                None,
                Some((2, false)),
                None,
            ],
        ),
        (
            32,
            vec![
                Some((1, false)),
                Some((0, true)),
                Some((32, false)),
                Some((0, true)),
                Some((17, false)),
                Some((0, true)),
                Some((16, false)),
                Some((0, true)),
                Some((9, false)),
                Some((0, true)),
                Some((24, false)),
                Some((0, true)),
                Some((25, false)),
                Some((0, true)),
                Some((8, false)),
                Some((0, true)),
                Some((5, false)),
                Some((0, true)),
                Some((28, false)),
                Some((0, true)),
                Some((21, false)),
                Some((0, true)),
                Some((12, false)),
                Some((0, true)),
                Some((13, false)),
                Some((0, true)),
                Some((20, false)),
                Some((0, true)),
                Some((29, false)),
                Some((0, true)),
                Some((4, false)),
                Some((0, true)),
                Some((3, false)),
                Some((0, true)),
                Some((30, false)),
                Some((0, true)),
                Some((19, false)),
                Some((0, true)),
                Some((14, false)),
                Some((0, true)),
                Some((11, false)),
                Some((0, true)),
                Some((22, false)),
                Some((0, true)),
                Some((27, false)),
                Some((0, true)),
                Some((6, false)),
                Some((0, true)),
                Some((7, false)),
                Some((0, true)),
                Some((26, false)),
                Some((0, true)),
                Some((23, false)),
                Some((0, true)),
                Some((10, false)),
                Some((0, true)),
                Some((15, false)),
                Some((0, true)),
                Some((18, false)),
                Some((0, true)),
                Some((31, false)),
                Some((0, true)),
                Some((2, false)),
            ],
        ),
    ] {
        let input = (1..input + 1).collect();
        assert_eq!(
            topbops_wasm::tournament::TournamentData::new(input, 0)
                .into_iter()
                .map(|n| n.map(|Node { item, disabled, .. }| (item, disabled)))
                .collect::<Vec<_>>(),
            output
        );
    }
}
