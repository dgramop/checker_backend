-- Anyone who has ever tapped into the MIX
CREATE TABLE members (
	-- GMU GNum
	gnum INT PRIMARY KEY NOT NULL,

	-- If the member is presently a staff member
	is_staff BOOLEAN NOT NULL

	-- Can add fields like NetID and Card number, if the user ever enters using those methods
);
