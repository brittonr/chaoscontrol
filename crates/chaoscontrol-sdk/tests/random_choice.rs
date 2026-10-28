use chaoscontrol_sdk::prelude::*;

#[test]
fn random_choice_matches_the_antithesis_slice_signature() {
    let empty: &[u8] = &[];
    assert_eq!(random_choice(empty), None);

    let single = [7u8];
    assert_eq!(random_choice(&single), Some(&7));

    let many = [1u8, 2, 3, 4, 5];
    let choice = random_choice(&many).expect("nonempty slice");
    assert!(many.contains(choice));
}

#[test]
fn random_choice_index_stays_within_the_bound() {
    assert_eq!(random_choice_index(1), 0);
    for _ in 0..32 {
        assert!(random_choice_index(7) < 7);
    }
}
