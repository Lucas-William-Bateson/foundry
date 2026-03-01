import MuiSwitch from "@mui/material/Switch";

interface SwitchProps {
  id?: string;
  checked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  className?: string;
}

function Switch({ id, checked, onCheckedChange, className }: SwitchProps) {
  return (
    <MuiSwitch
      id={id}
      size="small"
      checked={checked}
      onChange={(_e, val) => onCheckedChange?.(val)}
      className={className}
    />
  );
}

export { Switch };
