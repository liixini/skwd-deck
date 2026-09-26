if not status is-interactive
    return
end

if not set -q __skwd_theme_path
    set -g __skwd_theme_path (status filename)
    set -g __skwd_theme_variables fish_color_normal fish_color_command fish_color_builtin fish_color_function fish_color_keyword fish_color_quote fish_color_redirection fish_color_end fish_color_error fish_color_param fish_color_valid_path fish_color_option fish_color_comment fish_color_selection fish_color_operator fish_color_escape fish_color_autosuggestion fish_color_cwd fish_color_cwd_root fish_color_user fish_color_host fish_color_host_remote fish_color_status fish_color_cancel fish_color_search_match fish_color_history_current fish_pager_color_progress fish_pager_color_background fish_pager_color_prefix fish_pager_color_completion fish_pager_color_description fish_pager_color_selected_background fish_pager_color_selected_prefix fish_pager_color_selected_completion fish_pager_color_selected_description fish_pager_color_secondary_background fish_pager_color_secondary_prefix fish_pager_color_secondary_completion fish_pager_color_secondary_description
    set -g __skwd_theme_globals
    set -g __skwd_theme_exports
    for name in $__skwd_theme_variables
        if set -q -g $name
            set -ga __skwd_theme_globals $name
            set -g __skwd_saved_$name $$name
            if set -q -gx $name
                set -ga __skwd_theme_exports $name
            end
        end
    end
end

function __skwd_refresh_theme --on-variable __skwd_theme_revision
    if test "$__skwd_theme_revision[1]" != off; and test -f "$__skwd_theme_path"
        source "$__skwd_theme_path"
    else
        for name in $__skwd_theme_variables
            if contains -- $name $__skwd_theme_globals
                set -l saved __skwd_saved_$name
                if contains -- $name $__skwd_theme_exports
                    set -gx $name $$saved
                else
                    set -gu $name $$saved
                end
                set -eg $saved
            else
                set -eg $name
            end
        end
        set -eg __skwd_theme_path __skwd_theme_variables __skwd_theme_globals __skwd_theme_exports
        functions -e __skwd_refresh_theme
    end
    commandline -f repaint 2>/dev/null
end

set -g fish_color_normal '{{colors.on_surface.default.hex}}'
set -g fish_color_command '{{colors.primary.default.hex}}'
set -g fish_color_builtin '{{colors.primary.default.hex}}'
set -g fish_color_function '{{colors.primary.default.hex}}'
set -g fish_color_keyword '{{colors.tertiary.default.hex}}'
set -g fish_color_quote '{{colors.tertiary.default.hex}}'
set -g fish_color_redirection '{{colors.tertiary.default.hex}}'
set -g fish_color_end '{{colors.outline.default.hex}}'
set -g fish_color_error '{{colors.error.default.hex}}'
set -g fish_color_param '{{colors.on_surface.default.hex}}'
set -g fish_color_valid_path '{{colors.primary.default.hex}}'
set -g fish_color_option '{{colors.tertiary.default.hex}}'
set -g fish_color_comment '{{colors.outline.default.hex}}'
set -g fish_color_selection '{{colors.on_primary.default.hex}}' --background='{{colors.primary.default.hex}}'
set -g fish_color_operator '{{colors.tertiary.default.hex}}'
set -g fish_color_escape '{{colors.tertiary.default.hex}}'
set -g fish_color_autosuggestion '{{colors.outline.default.hex}}'
set -g fish_color_cwd '{{colors.primary.default.hex}}'
set -g fish_color_cwd_root '{{colors.error.default.hex}}'
set -g fish_color_user '{{colors.primary.default.hex}}'
set -g fish_color_host '{{colors.tertiary.default.hex}}'
set -g fish_color_host_remote '{{colors.tertiary.default.hex}}'
set -g fish_color_status '{{colors.error.default.hex}}'
set -g fish_color_cancel '{{colors.error.default.hex}}'
set -g fish_color_search_match '{{colors.on_primary.default.hex}}' --background='{{colors.primary.default.hex}}'
set -g fish_color_history_current '{{colors.primary.default.hex}}'
set -g fish_pager_color_progress '{{colors.primary.default.hex}}'
set -g fish_pager_color_background --background='{{colors.surface.default.hex}}'
set -g fish_pager_color_prefix '{{colors.primary.default.hex}}'
set -g fish_pager_color_completion '{{colors.on_surface.default.hex}}'
set -g fish_pager_color_description '{{colors.outline.default.hex}}'
set -g fish_pager_color_selected_background --background='{{colors.primary.default.hex}}'
set -g fish_pager_color_selected_prefix '{{colors.on_primary.default.hex}}'
set -g fish_pager_color_selected_completion '{{colors.on_primary.default.hex}}'
set -g fish_pager_color_selected_description '{{colors.on_primary.default.hex}}'
set -g fish_pager_color_secondary_background --background='{{colors.surface_container.default.hex}}'
set -g fish_pager_color_secondary_prefix '{{colors.primary.default.hex}}'
set -g fish_pager_color_secondary_completion '{{colors.on_surface.default.hex}}'
set -g fish_pager_color_secondary_description '{{colors.outline.default.hex}}'
