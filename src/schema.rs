// @generated automatically by Diesel CLI.

diesel::table! {
    members (gnum) {
        gnum -> Integer,
        is_staff -> Bool,
    }
}

diesel::table! {
    taken (id) {
        id -> Text,
        member -> Integer,
        workshop -> Text,
    }
}

diesel::table! {
    workshops (id) {
        id -> Text,
        name -> Text,
    }
}

diesel::joinable!(taken -> members (member));
diesel::joinable!(taken -> workshops (workshop));

diesel::allow_tables_to_appear_in_same_query!(
    members,
    taken,
    workshops,
);
