/// "Mark as decorative" starts checked when every selected shape is already decorative.
pub(super) fn alt_text_starts_decorative(decorative: impl IntoIterator<Item = bool>) -> bool {
    let mut any = false;
    for on in decorative {
        if !on {
            return false;
        }
        any = true;
    }
    any
}
