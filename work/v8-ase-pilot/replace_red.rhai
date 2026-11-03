// M9 acceptance: open fixture, replace one color, save
open(params["in"]);
replace_color("#ff4040", params["to"]);
save(params["out"]);
