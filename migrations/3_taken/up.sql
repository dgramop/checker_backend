-- Relationship: A student has taken a workshop
CREATE TABLE taken (
	id TEXT PRIMARY KEY NOT NULL,
	
	member INT NOT NULL,
	workshop TEXT NOT NULL,

	FOREIGN KEY (member) REFERENCES members(gnum),
	FOREIGN KEY (workshop) REFERENCES workshops(id),
	UNIQUE (member,workshop)
	-- Assumption: A member may take the same workshop multiple times with different folks
	-- Warning: A staff member may stop working at the MIX, at which point there may still be students who took their workshop. Do not assume teacher is a present staff member 
);
