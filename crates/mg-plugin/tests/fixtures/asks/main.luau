local mg = require("@moonglow")

-- By itself, when the module is saved: asks first.
mg.on("noted", function(ctx)
    if ctx.ui:confirm("Note this save?") then
        ctx.log:info("noted")
    else
        ctx.log:info("not noted")
    end
end)
